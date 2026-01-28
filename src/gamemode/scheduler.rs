use std::collections::HashMap;

use color_eyre::eyre;
use slotmap::{SlotMap, new_key_type};

use crate::gamemode::{
    api::protocol::{AsyncTaskResult, GameModePlugin},
    utils::LuaResultExt,
};

new_key_type! {
    struct TaskId;
}

pub enum SchedulerMessage {
    Wake {
        task_id: TaskId,
        result: AsyncTaskResult,
    },
    Cancel(TaskId),
    Error {
        task_id: TaskId,
        err: String,
    },
}

pub struct SchedulerParams {
    pub channel_buffer: usize,
    pub plugins: HashMap<String, Box<dyn GameModePlugin>>,
}

pub struct Scheduler {
    threads: SlotMap<TaskId, mlua::Thread>,
    rx: flume::Receiver<SchedulerMessage>,
    tx: flume::Sender<SchedulerMessage>,
    plugins: HashMap<String, Box<dyn GameModePlugin>>,
}
impl Scheduler {
    pub fn new(params: SchedulerParams) -> Self {
        let (tx, rx) = flume::bounded::<SchedulerMessage>(params.channel_buffer);

        Self {
            threads: SlotMap::<TaskId, mlua::Thread>::with_key(),
            rx,
            tx,
            plugins: params.plugins,
        }
    }

    pub fn add_task(&mut self, thread: mlua::Thread) -> eyre::Result<()> {
        let task_id = self.threads.insert(thread);

        if let Some(t) = self.threads.get(task_id) {
            self.process_coroutine(task_id, t)?;
        }

        Ok(())
    }

    fn advance_task(&mut self, task_id: TaskId) -> eyre::Result<()> {
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
            let (prefix, suffix) = opcode
                .split_once("_")
                .ok_or_else(|| eyre::eyre!("Invalid format for opcode: {}", opcode))?;

            if let Some(plugin) = self.plugins.get(prefix) {
                let args: mlua::Table = t
                    .get("args")
                    .wrap_err("process_coroutine: `args` not found")?;
                let tx = self.tx.clone();

                let Some(task) = plugin.handle_op(suffix, args)? else {
                    return Ok(());
                };
                tokio::spawn(async move {
                    match task.await {
                        Ok(result) => tx.send(SchedulerMessage::Wake { task_id, result }),
                        Err(err) => tx.send(SchedulerMessage::Error {
                            task_id,
                            err: format!("Opcode {} failed to execute: {}", opcode, err),
                        }),
                    }
                });
            }
        }

        Ok(())
    }

    pub fn tick(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                SchedulerMessage::Cancel(task_id) => {
                    self.threads.remove(task_id);
                }
                SchedulerMessage::Wake { task_id, payload } => {}
                SchedulerMessage::Error { task_id, err } => {
                    println!(
                        "scheduler.tick: scene execution error. task_id = {:?}; {}",
                        &task_id, err
                    );
                    self.threads.remove(task_id);
                }
            }
        }
    }
}
