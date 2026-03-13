use std::cell::RefCell;

use color_eyre::eyre;
use mlua::{IntoLuaMulti, Lua};

use crate::runtime::{
    app_data::{self, ExecutionContext},
    plugins::on::protocol::GameModeEvent,
    utils::LuaResultExt,
};

pub type SequenceId = u64;

struct QueuedEvent {
    created_at_seq: SequenceId,
    event: GameModeEvent,
}
struct PendingHandle {
    context: ExecutionContext,
    args: mlua::MultiValue,
    func: mlua::Function,
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
        let mut inner = self.inner.borrow_mut();

        let seq = inner.sequence;
        inner.sequence += 1;
        inner.queue.push(QueuedEvent {
            created_at_seq: seq,
            event,
        });
    }

    fn emit(&self, q_event: QueuedEvent, lua: &Lua) -> eyre::Result<()> {
        let event = q_event.event;
        let scopes = event.scopes.clone();
        let key = event.data.key();

        let pending_handles = {
            let mut listeners = match lua.app_data_mut::<app_data::EventListeners>() {
                Some(d) => d,
                None => return Err(eyre::eyre!("App data is not initialized")),
            };

            let listeners_for_key = match listeners.get_mut(&key) {
                Some(fns) => fns,
                None => return Ok(()),
            };
            if listeners_for_key.is_empty() {
                return Ok(());
            }

            let args = event
                .into_lua_multi(lua)
                .wrap_err("Failed to materialize args")?;

            let mut pending_handles: Vec<PendingHandle> = Vec::new();

            for listener in listeners_for_key
                .iter_mut()
                .filter(|l| scopes.contains(&l.scope))
            {
                if let Some(args) = listener.call(q_event.created_at_seq, args.clone())? {
                    pending_handles.push(PendingHandle {
                        context: listener.context,
                        args,
                        func: listener.handle.clone(),
                    });
                } else {
                    continue;
                }
            }
            listeners_for_key.retain(|l| !l.is_exhausted());

            pending_handles
        };

        for handle in pending_handles {
            let result = handle.func.call::<Option<bool>>(handle.args);
            match handle.context {
                ExecutionContext::Global => {
                    let result = result
                        .wrap_err(format!("Error in `{:?}` event listener", &key).as_str())?
                        .unwrap_or(false);
                    if result {
                        return Ok(());
                    }
                }
                ExecutionContext::Impromptu => {
                    // TODO: propagate the error to commit router? to logger?
                    //       how to know if such event failed from the outside.
                    //       delete a failed listener from listeners list
                    eprintln!(
                        "Error in `{:?}` event listener (registered during the impromptu)",
                        &key
                    );

                    if let Ok(result) = result {
                        if result.unwrap_or(false) {
                            return Ok(());
                        }
                    }
                }
            }
        }

        Ok(())
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
