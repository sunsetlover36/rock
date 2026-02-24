use std::cell::RefCell;

use color_eyre::eyre;
use mlua::{IntoLuaMulti, Lua};

use crate::runtime::{api::on::GameModeEvent, app_data, utils::LuaResultExt};

struct QueuedEvent {
    created_at_seq: u64,
    event: GameModeEvent,
}
struct PendingHandle {
    args: mlua::MultiValue,
    func: mlua::Function,
}

struct EventBusInner {
    queue: Vec<QueuedEvent>,
    sequence: u64,
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

    pub fn increment_sequence(&self) -> u64 {
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
            let listeners = match listeners.get_mut(&key) {
                Some(fns) => fns,
                None => return Ok(()),
            };
            if listeners.is_empty() {
                return Ok(());
            }

            let args = event
                .into_lua_multi(lua)
                .wrap_err("Failed to materialize args")?;

            let mut pending_handles: Vec<PendingHandle> = Vec::new();
            for listener in listeners.iter_mut().filter(|l| scopes.contains(&l.scope)) {
                if listener.created_at_seq > q_event.created_at_seq || listener.limit_reached() {
                    continue;
                }

                let handle_args = listener.process_chain(args.clone())?;
                if let Some(args) = handle_args {
                    listener.call_count += 1;
                    pending_handles.push(PendingHandle {
                        args,
                        func: listener.handle.clone(),
                    });
                } else {
                    continue;
                }
            }
            listeners.retain(|l| !l.limit_reached());

            pending_handles
        };

        for handle in pending_handles {
            handle
                .func
                .call::<()>(handle.args)
                .wrap_err(format!("Error in `{:?}` event listener", &key).as_str())?;
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
