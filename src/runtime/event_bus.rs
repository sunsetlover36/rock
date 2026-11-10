use std::cell::RefCell;

use color_eyre::eyre;
use mlua::{IntoLuaMulti, Lua};

use crate::runtime::{
    app_data::{self, ExecutionContext},
    plugins::{
        layer::{LayerContextGuard, layer_is_alive},
        on::{GameModeListener, protocol::GameModeEvent},
    },
    utils::LuaResultExt,
};

pub type SequenceId = u64;

struct QueuedEvent {
    created_at_seq: SequenceId,
    event: GameModeEvent,
}
struct PendingHandle {
    listener: GameModeListener,
    args: mlua::MultiValue,
}

struct ListenerContextGuard<'lua> {
    lua: &'lua Lua,
    previous_context: ExecutionContext,
    _layer: LayerContextGuard<'lua>,
}
impl<'lua> ListenerContextGuard<'lua> {
    fn enter(lua: &'lua Lua, listener: &GameModeListener) -> mlua::Result<Self> {
        let previous_context = *crate::runtime::get_app_data::<ExecutionContext>(lua)?;
        let layer = LayerContextGuard::enter(lua, listener.owner)?;
        lua.set_app_data(listener.context);
        Ok(Self {
            lua,
            previous_context,
            _layer: layer,
        })
    }
}
impl Drop for ListenerContextGuard<'_> {
    fn drop(&mut self) {
        self.lua.set_app_data(self.previous_context);
    }
}

fn listener_is_active(lua: &Lua, listener: &GameModeListener) -> mlua::Result<bool> {
    if !listener.active.get() {
        return Ok(false);
    }
    match listener.owner {
        Some(owner) => layer_is_alive(lua, owner),
        None => Ok(true),
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
        });
    }

    fn emit(&self, q_event: QueuedEvent, lua: &Lua) -> eyre::Result<()> {
        let event = q_event.event;
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
                if !listener_is_active(lua, &listener).wrap_err("Failed to check listener owner")? {
                    continue;
                }
                let _guard = ListenerContextGuard::enter(lua, &listener)
                    .wrap_err("Failed to enter listener context")?;
                if let Some(args) = listener.call(q_event.created_at_seq, args.clone())? {
                    pending_handles.push(PendingHandle {
                        listener: listener.clone(),
                        args,
                    });
                }
            }

            // Completion removes subscriptions, but the last accepted event
            // still gets its handler. Cancellation tokens remain available to
            // layer cleanup and ListenerHandle::off even after this removal.
            if let Some(mut listeners) = lua.app_data_mut::<app_data::EventListeners>() {
                if let Some(listeners) = listeners.0.get_mut(&key) {
                    listeners.retain(|listener| !listener.is_exhausted());
                }
            }

            for handle in pending_handles {
                if !listener_is_active(lua, &handle.listener)
                    .wrap_err("Failed to check listener owner")?
                {
                    continue;
                }
                let _guard = ListenerContextGuard::enter(lua, &handle.listener)
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
                listeners.retain(|listener| !listener.is_exhausted());
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
