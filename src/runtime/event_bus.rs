use std::cell::RefCell;

use color_eyre::eyre;
use mlua::{IntoLuaMulti, Lua};

use crate::runtime::{api::on::GameModeEventData, app_data, utils::LuaResultExt};

struct QueuedEvent {
    created_at_seq: u64,
    data: GameModeEventData,
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

    pub fn schedule_event(&self, data: GameModeEventData) {
        let mut inner = self.inner.borrow_mut();

        let seq = inner.sequence;
        inner.sequence += 1;
        inner.queue.push(QueuedEvent {
            created_at_seq: seq,
            data,
        });
    }

    fn emit(&self, event: QueuedEvent, lua: &Lua) -> eyre::Result<()> {
        let key = event.data.key();

        let (pending_handles, args) = {
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

            let mut pending_handles = Vec::new();
            let args = event
                .data
                .into_lua_multi(lua)
                .wrap_err("Failed to materialize args")?;

            for listener in listeners.iter_mut() {
                if listener.created_at_seq > event.created_at_seq
                    || listener.limit_reached()
                    || !listener.passes_filters(&args)?
                {
                    continue;
                }

                listener.call_count += 1;
                pending_handles.push(listener.handle.clone())
            }
            listeners.retain(|l| !l.limit_reached());

            (pending_handles, args)
        };

        for handle in pending_handles {
            handle
                .call::<()>(&args)
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
