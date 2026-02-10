use std::collections::HashMap;

use color_eyre::eyre;
use mlua::Lua;
use slotmap::{SlotMap, new_key_type};

use crate::{
    runtime::{
        api::protocol::{AsyncTaskResult, GameModePlugin},
        utils::LuaResultExt,
    },
    utils::json_to_lua,
};

new_key_type! {
    pub struct TaskId;
}

#[derive(Debug)]
pub enum SchedulerMessage {
    AddTask(mlua::RegistryKey),
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
    pub plugins: HashMap<String, Box<dyn GameModePlugin>>,
    pub rx: flume::Receiver<SchedulerMessage>,
    pub tx: flume::Sender<SchedulerMessage>,
    pub tokio_handle: tokio::runtime::Handle,
}

pub struct Scheduler {
    threads: SlotMap<TaskId, mlua::RegistryKey>,
    rx: flume::Receiver<SchedulerMessage>,
    tx: flume::Sender<SchedulerMessage>,
    tokio_handle: tokio::runtime::Handle,
    plugins: HashMap<String, Box<dyn GameModePlugin>>,
}
impl Scheduler {
    pub fn new(params: SchedulerParams) -> Self {
        Self {
            threads: SlotMap::<TaskId, mlua::RegistryKey>::with_key(),
            rx: params.rx,
            tx: params.tx,
            tokio_handle: params.tokio_handle,
            plugins: params.plugins,
        }
    }

    fn add_task(&mut self, lua: &Lua, thread_rk: mlua::RegistryKey) -> eyre::Result<()> {
        let task_id = self.threads.insert(thread_rk);
        if let Err(e) = self.advance_task(lua, task_id, ()) {
            self.threads.remove(task_id);
            return Err(e);
        }

        Ok(())
    }

    fn advance_task(
        &mut self,
        lua: &Lua,
        task_id: TaskId,
        args: impl mlua::IntoLuaMulti,
    ) -> eyre::Result<()> {
        let thread_rk = match self.threads.get(task_id) {
            Some(t) => t,
            None => return Ok(()),
        };
        let thread: mlua::Thread = lua
            .registry_value(thread_rk)
            .wrap_err("scheduler.advance_task: cannot find a thread by its registry key")?;

        let resume_result = thread.resume::<mlua::Value>(args);
        match resume_result {
            Err(e) => {
                return Err(eyre::eyre!(
                    "scheduler.advance_task: task {:?} crashed: {}",
                    task_id,
                    e
                ));
            }
            Ok(yielded_val) => match thread.status() {
                mlua::ThreadStatus::Resumable => {
                    self.handle_yield(task_id, yielded_val)?;
                }
                _ => {
                    self.threads.remove(task_id);
                }
            },
        }

        Ok(())
    }

    fn handle_yield(&self, task_id: TaskId, yielded_val: mlua::Value) -> eyre::Result<()> {
        let t = match yielded_val {
            mlua::Value::Table(t) => t,
            _ => {
                return Err(eyre::eyre!(
                    "handle_yield: encountered an unexpected yield output"
                ));
            }
        };

        let opcode: String = t
            .get("opcode")
            .wrap_err("handle_yield: cannot find `opcode` in the yield output")?;
        let (prefix, suffix) = opcode
            .split_once("_")
            .ok_or_else(|| eyre::eyre!("handle_yield: invalid format for opcode ({})", opcode))?;

        match self.plugins.get(&prefix.to_lowercase()) {
            Some(plugin) => {
                let args: mlua::Table = t
                    .get("args")
                    .wrap_err("handle_yield: cannot find `args` in the yield output")?;

                let Some(task) = plugin.handle_op(suffix, args)? else {
                    return Ok(());
                };
                let tx = self.tx.clone();
                self.tokio_handle.spawn(async move {
                    match task.await {
                        Ok(result) => {
                            let _ = tx.send(SchedulerMessage::Wake { task_id, result });
                        }
                        Err(err) => {
                            let _ = tx.send(SchedulerMessage::Error {
                                task_id,
                                err: err.to_string(),
                            });
                        }
                    }
                });
            }
            None => return Err(eyre::eyre!("handle_yield: plugin {} not found", prefix)),
        }

        Ok(())
    }

    pub fn tick(&mut self, lua: &Lua) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                SchedulerMessage::AddTask(thread_rk) => {
                    if let Err(e) = self.add_task(lua, thread_rk) {
                        eprintln!("scheduler.tick: error adding task ({})", e);
                    }
                }
                SchedulerMessage::Cancel(task_id) => {
                    self.threads.remove(task_id);
                }
                SchedulerMessage::Wake { task_id, result } => {
                    let lua_val = match result {
                        AsyncTaskResult::JsonValue(v) => match json_to_lua(lua, v) {
                            mlua::Result::Ok(v) => v,
                            mlua::Result::Err(_) => continue,
                        },
                        AsyncTaskResult::Text(s) => match lua.create_string(&s) {
                            mlua::Result::Ok(s) => mlua::Value::String(s),
                            mlua::Result::Err(_) => continue,
                        },
                        AsyncTaskResult::Nil => mlua::Value::Nil,
                    };

                    if let Err(e) = self.advance_task(lua, task_id, lua_val) {
                        eprintln!("scheduler.tick: error advancing task {:?} ({})", task_id, e);
                        self.threads.remove(task_id);
                    };
                }
                SchedulerMessage::Error { task_id, err } => {
                    eprintln!(
                        "scheduler.tick: scene execution error. task_id = {:?}; {}",
                        &task_id, err
                    );
                    self.threads.remove(task_id);
                }
            }
        }
    }
}
