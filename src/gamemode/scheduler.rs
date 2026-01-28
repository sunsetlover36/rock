use color_eyre::eyre;
use slotmap::{SlotMap, new_key_type};

use crate::{
    gamemode::{api::YielderRawOutput, utils::LuaResultExt},
    meta_db::MetaValue,
};

new_key_type! {
    struct TaskId;
}

pub enum WakePayload {
    MetaDb(MetaValue),
}
pub enum SchedulerMessage {
    AddThread(mlua::Thread),
    Wake {
        task_id: TaskId,
        payload: WakePayload,
    },
    Cancel(TaskId),
}

pub struct Scheduler {
    threads: SlotMap<TaskId, mlua::Thread>,
    rx: flume::Receiver<SchedulerMessage>,
    tx: flume::Sender<SchedulerMessage>,
}
impl Scheduler {
    pub fn new(buffer: usize) -> Self {
        let (tx, rx) = flume::bounded::<SchedulerMessage>(buffer);

        Self {
            threads: SlotMap::<TaskId, mlua::Thread>::with_key(),
            rx,
            tx,
        }
    }

    pub fn process_coroutine(&self, thread: mlua::Thread) -> eyre::Result<()> {
        if let Some(yielded) = thread.resume::<mlua::Value>(()).ok() {
            let t = match yielded {
                mlua::Value::Table(t) => t,
                _ => {
                    return Err(eyre::eyre!(
                        "process_coroutine: encountered an unexpected yield output"
                    ));
                }
            };
            let opcode: String = t
                .get("opcode")
                .wrap_err("process_coroutine: `opcode` not found")?;
            let args: mlua::Table = t
                .get("args")
                .wrap_err("process_coroutine: `args` not found")?;
        }

        Ok(())
    }

    pub fn start(mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                SchedulerMessage::AddThread(thread) => {
                    let _ = thread.resume::<()>(());
                    self.threads.insert(thread);
                }
                SchedulerMessage::Cancel(task_id) => {
                    self.threads.remove(task_id);
                }
                SchedulerMessage::Wake { task_id, payload } => {}
            }
        }
    }
}
