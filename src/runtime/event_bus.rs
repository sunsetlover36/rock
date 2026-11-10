use std::cell::RefCell;

use color_eyre::eyre;
use mlua::{IntoLuaMulti, Lua};

use crate::runtime::{
    app_data::{self, ExecutionContext},
    lifetime::{Cancellation, OwnedWork, WorkContext, WorkContextGuard},
    plugins::on::{
        GameModeListener,
        protocol::{GameModeEvent, GameModeEventData, PlayerEventData},
    },
    utils::LuaResultExt,
};

pub type SequenceId = u64;

struct QueuedEvent {
    created_at_seq: SequenceId,
    event: GameModeEvent,
    source: Option<(OwnedWork, bool)>,
}
struct PendingHandle {
    listener: GameModeListener,
    args: mlua::MultiValue,
    context: WorkContext,
}

struct FinishEvent(Option<Cancellation>);
impl Drop for FinishEvent {
    fn drop(&mut self) {
        if let Some(cancel) = &self.0 {
            cancel.cancel();
        }
    }
}

fn listener_context(
    listener: &GameModeListener,
    player: Option<rock_wire::PlayerKey>,
    source: Option<&OwnedWork>,
) -> WorkContext {
    let mut players = listener.players.clone();
    for pk in source
        .into_iter()
        .flat_map(|s| s.context.players.iter().copied())
        .chain(player)
    {
        if !players.contains(&pk) {
            players.push(pk);
        }
    }
    WorkContext {
        layer: listener
            .owner
            .or_else(|| source.and_then(|s| s.context.layer)),
        players,
        execution: listener.context,
    }
}

struct EventBusInner {
    queue: Vec<QueuedEvent>,
    sequence: SequenceId,
}
pub(crate) struct EventBus {
    inner: RefCell<EventBusInner>,
}
impl EventBus {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(EventBusInner {
                queue: Vec::new(),
                sequence: 0,
            }),
        }
    }

    pub fn increment_sequence(&self) -> SequenceId {
        let mut inner = self.inner.borrow_mut();
        inner.sequence += 1;
        inner.sequence
    }

    pub fn schedule_event(&self, event: GameModeEvent) {
        let created_at_seq = self.increment_sequence();

        let mut inner = self.inner.borrow_mut();
        inner.queue.push(QueuedEvent {
            created_at_seq,
            event,
            source: None,
        });
    }

    pub(crate) fn schedule_owned_event(
        &self,
        event: GameModeEvent,
        source: OwnedWork,
        finish: bool,
    ) {
        let created_at_seq = self.increment_sequence();
        self.inner.borrow_mut().queue.push(QueuedEvent {
            created_at_seq,
            event,
            source: Some((source, finish)),
        });
    }

    fn emit(&self, q_event: QueuedEvent, lua: &Lua) -> eyre::Result<()> {
        let source = q_event.source.as_ref().map(|(work, _)| work);
        let _finish = FinishEvent(
            q_event
                .source
                .as_ref()
                .filter(|(_, finish)| *finish)
                .map(|(work, _)| work.cancel.clone()),
        );
        if let Some(source) = source {
            if !source
                .is_alive(lua)
                .wrap_err("Failed to check event lifetime")?
            {
                return Ok(());
            }
        }
        let event = q_event.event;
        let player = match &event.data {
            GameModeEventData::Player(
                PlayerEventData::Online { player, .. }
                | PlayerEventData::Input { player, .. }
                | PlayerEventData::Enter { player, .. }
                | PlayerEventData::Exit { player, .. }
                | PlayerEventData::Signal { player, .. },
            ) => Some(player.key()),
            _ => None,
        };
        let scopes = event.scopes.clone();
        let key = event.data.key();

        // Snapshot registration identities, sharing each chain's state. Lua
        // operators may register/remove listeners or clear layers, so never
        // hold a listener-registry borrow while running Lua code.
        let listeners_for_event = {
            let listeners = crate::runtime::get_app_data::<app_data::EventListeners>(lua)
                .wrap_err("Event listeners are not initialized")?;
            listeners
                .0
                .get(&key)
                .into_iter()
                .flatten()
                .filter(|listener| scopes.contains(&listener.scope))
                .cloned()
                .collect::<Vec<_>>()
        };
        if listeners_for_event.is_empty() {
            return Ok(());
        }

        let args = event
            .into_lua_multi(lua)
            .wrap_err("Failed to materialize args")?;
        let result = (|| -> eyre::Result<()> {
            let mut pending_handles = Vec::new();
            for listener in listeners_for_event {
                let context = listener_context(&listener, player, source);
                let active = || {
                    listener.active.get()
                        && context.is_alive(lua).unwrap_or(false)
                        && source.is_none_or(|source| source.cancel.get())
                };
                if !active() {
                    continue;
                }
                let _guard = WorkContextGuard::enter(lua, &context)
                    .wrap_err("Failed to enter listener context")?;
                if let Some(args) = listener.call(q_event.created_at_seq, args.clone(), active)? {
                    pending_handles.push(PendingHandle {
                        listener: listener.clone(),
                        args,
                        context: context.clone(),
                    });
                }
            }

            // Completion removes subscriptions, but the last accepted event
            // still gets its handler. Cancellation tokens remain available to
            // layer cleanup and ListenerHandle::off even after this removal.
            if let Some(mut listeners) = lua.app_data_mut::<app_data::EventListeners>() {
                if let Some(listeners) = listeners.0.get_mut(&key) {
                    listeners.retain(|listener| listener.active.get() && !listener.is_exhausted());
                }
            }

            for handle in pending_handles {
                if !handle.listener.active.get()
                    || !handle
                        .context
                        .is_alive(lua)
                        .wrap_err("Failed to check listener context")?
                    || source.is_some_and(|source| !source.cancel.get())
                {
                    continue;
                }
                let _guard = WorkContextGuard::enter(lua, &handle.context)
                    .wrap_err("Failed to enter listener context")?;
                let result = handle.listener.handle.call::<Option<bool>>(handle.args);
                match handle.listener.context {
                    ExecutionContext::Global => {
                        if result
                            .wrap_err(format!("Error in `{:?}` event listener", &key).as_str())?
                            .unwrap_or(false)
                        {
                            return Ok(());
                        }
                    }
                    ExecutionContext::Impromptu => {
                        // TODO: propagate listener errors to the live-code caller.
                        if let Err(err) = &result {
                            eprintln!(
                                "Error in `{:?}` event listener (registered during the impromptu): {err}",
                                &key
                            );
                        }
                        if result.is_ok_and(|result| result.unwrap_or(false)) {
                            return Ok(());
                        }
                    }
                }
            }
            Ok(())
        })();

        // Also retire exhausted chains when a later operator/handler failed.
        if let Some(mut listeners) = lua.app_data_mut::<app_data::EventListeners>() {
            if let Some(listeners) = listeners.0.get_mut(&key) {
                listeners.retain(|listener| listener.active.get() && !listener.is_exhausted());
            }
        }
        result
    }

    pub fn flush(&self, lua: &Lua) -> eyre::Result<()> {
        let events = {
            let mut inner = self.inner.borrow_mut();
            std::mem::take(&mut inner.queue)
        };

        for event in events {
            self.emit(event, lua)?;
        }

        Ok(())
    }
}
