use std::{collections::HashMap, time::Duration};

use color_eyre::eyre;
use slotmap::SlotMap;

use crate::{
    runtime::{
        plugins::protocol::{AsyncTaskResult, GameModePlugin, PluginName, YieldKind},
        utils::LuaResultExt,
    },
    utils::json_to_lua,
};

use super::ctx::SceneCtx;
use super::protocol::{SceneManagerMessage, SceneYieldOp, TaskId};

struct SceneTask {
    thread_rk: mlua::RegistryKey,
    label: String,
    waiting_on: Option<String>,
    work: crate::runtime::lifetime::OwnedWork,
}

pub struct SceneManagerParams {
    pub plugins: HashMap<PluginName, Box<dyn GameModePlugin>>,
    pub rx: flume::Receiver<SceneManagerMessage>,
    pub tx: flume::Sender<SceneManagerMessage>,
    pub tokio_handle: tokio::runtime::Handle,
}

pub struct SceneManager {
    threads: SlotMap<TaskId, SceneTask>,
    tx: flume::Sender<SceneManagerMessage>,
    rx: flume::Receiver<SceneManagerMessage>,
    tokio_handle: tokio::runtime::Handle,
    plugins: HashMap<PluginName, Box<dyn GameModePlugin>>,
}
impl SceneManager {
    pub fn new(params: SceneManagerParams) -> Self {
        Self {
            threads: SlotMap::<TaskId, SceneTask>::with_key(),
            rx: params.rx,
            tx: params.tx,
            tokio_handle: params.tokio_handle,
            plugins: params.plugins,
        }
    }

    fn add_task(
        &mut self,
        lua: &mlua::Lua,
        thread_rk: mlua::RegistryKey,
        label: String,
        work: crate::runtime::lifetime::OwnedWork,
    ) -> eyre::Result<()> {
        if !work
            .is_alive(lua)
            .wrap_err("Failed to check scene lifetime")?
        {
            work.cancel.cancel();
            return Ok(());
        }
        let task_id = self.threads.insert(SceneTask {
            thread_rk,
            label,
            waiting_on: None,
            work,
        });
        if let Err(e) = self.advance_task(lua, task_id, SceneCtx {}) {
            self.log_task_error(lua, task_id, "starting scene", &format!("{:#}", e));
            self.threads.remove(task_id);
            return Err(e);
        }

        Ok(())
    }

    fn advance_task(
        &mut self,
        lua: &mlua::Lua,
        task_id: TaskId,
        args: impl mlua::IntoLuaMulti,
    ) -> eyre::Result<()> {
        let Some(work) = self.threads.get(task_id).map(|task| task.work.clone()) else {
            return Ok(());
        };
        if !work
            .is_alive(lua)
            .wrap_err("Failed to check scene lifetime")?
        {
            self.threads.remove(task_id);
            return Ok(());
        }
        let _guard = crate::runtime::lifetime::WorkContextGuard::enter(lua, &work.context)
            .wrap_err("Failed to enter scene context")?;
        let thread: mlua::Thread = {
            let task = match self.threads.get(task_id) {
                Some(task) => task,
                None => return Ok(()),
            };
            lua.registry_value(&task.thread_rk)
                .wrap_err("Failed to resume scene: coroutine registry key is invalid")?
        };

        let resume_result = thread.resume::<mlua::Value>(args);
        if !work
            .is_alive(lua)
            .wrap_err("Failed to check scene lifetime")?
        {
            self.threads.remove(task_id);
            return Ok(());
        }
        match resume_result {
            Err(e) => {
                return Err(eyre::eyre!("Lua scene function failed: {}", e));
            }
            Ok(yielded_val) => match thread.status() {
                mlua::ThreadStatus::Resumable => {
                    self.handle_yield(lua, task_id, yielded_val)?;
                }
                _ => {
                    self.threads.remove(task_id);
                }
            },
        }

        Ok(())
    }

    fn label_for_plugin_op(prefix: &str, suffix: &str) -> String {
        format!("{}.{}", prefix.to_lowercase(), suffix.to_lowercase())
    }

    fn set_waiting_on(&mut self, task_id: TaskId, label: impl Into<String>) {
        if let Some(task) = self.threads.get_mut(task_id) {
            task.waiting_on = Some(label.into());
        }
    }

    fn waiting_phase(&self, task_id: TaskId, default: &str) -> String {
        self.threads
            .get(task_id)
            .and_then(|task| task.waiting_on.as_ref())
            .map(|waiting_on| format!("{default} after `{waiting_on}`"))
            .unwrap_or_else(|| default.to_owned())
    }

    fn indent_error(err: &str) -> String {
        err.lines()
            .map(|line| format!("    {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn log_task_error(&self, lua: &mlua::Lua, task_id: TaskId, phase: &str, err: &str) {
        if let Some(task) = self.threads.get(task_id) {
            crate::runtime::diagnostics::record_error(
                lua,
                task.work.context.origin.as_ref(),
                crate::runtime::diagnostics::Phase::Scene,
                format!("{}: {err}", phase),
                None,
            );
            let waiting = task
                .waiting_on
                .as_ref()
                .map(|waiting_on| format!("\n  waiting on: {waiting_on}"))
                .unwrap_or_default();
            eprintln!(
                "[SCENE] {}\n  task: {:?}\n  phase: {}{}\n  error:\n{}",
                task.label,
                task_id,
                phase,
                waiting,
                Self::indent_error(err)
            );
        } else {
            eprintln!(
                "[SCENE] Unknown scene task {:?}\n  phase: {}\n  error:\n{}",
                task_id,
                phase,
                Self::indent_error(err)
            );
        }
    }

    fn handle_plugin_yield(
        &mut self,
        lua: &mlua::Lua,
        task_id: TaskId,
        op: mlua::Table,
    ) -> eyre::Result<()> {
        let opcode: String = op
            .get("opcode")
            .wrap_err("handle_plugin_yield: cannot find `opcode` in the yield output")?;
        let (prefix, suffix) = opcode.split_once("_").ok_or_else(|| {
            eyre::eyre!(
                "handle_plugin_yield: invalid format for opcode ({})",
                opcode
            )
        })?;
        self.set_waiting_on(task_id, Self::label_for_plugin_op(prefix, suffix));

        let plugin_name = prefix.to_lowercase().parse::<PluginName>()?;
        match self.plugins.get(&plugin_name) {
            Some(plugin) => {
                let args = op.get("args").unwrap_or(mlua::Value::Nil);
                let Some(task) = plugin.handle_op(lua, suffix, args)? else {
                    return Err(eyre::eyre!(
                        "Scene yielded `{}` but plugin `{}` did not create an async task",
                        opcode,
                        plugin_name
                    ));
                };
                let tx = self.tx.clone();
                let handle = self.tokio_handle.spawn(async move {
                    match task.await {
                        Ok(result) => {
                            let _ = tx
                                .send_async(SceneManagerMessage::Wake { task_id, result })
                                .await;
                        }
                        Err(err) => {
                            let _ = tx
                                .send_async(SceneManagerMessage::Error {
                                    task_id,
                                    err: format!("{:#}", err),
                                })
                                .await;
                        }
                    }
                });
                self.threads[task_id]
                    .work
                    .cancel
                    .set_abort(handle.abort_handle());
            }
            None => {
                return Err(eyre::eyre!(
                    "handle_plugin_yield: plugin {} not found",
                    prefix
                ));
            }
        }

        Ok(())
    }
    fn handle_scene_yield(
        &mut self,
        _: &mlua::Lua,
        task_id: TaskId,
        op: mlua::Table,
    ) -> eyre::Result<()> {
        let opcode: String = op
            .get("opcode")
            .wrap_err("handle_scene_yield: cannot find `opcode` in the yield output")?;
        let scene_opcode = opcode.parse::<SceneYieldOp>()?;
        match scene_opcode {
            SceneYieldOp::Sleep => {
                self.set_waiting_on(task_id, "scene.sleep");
                let seconds: u64 = op
                    .get("args")
                    .wrap_err("handle_scene_yield: cannot find `args` in the yield output")?;
                if std::time::Instant::now()
                    .checked_add(Duration::from_secs(seconds))
                    .is_none()
                {
                    return Err(eyre::eyre!("scene.sleep: duration is too large"));
                }
                let tx = self.tx.clone();
                let handle = self.tokio_handle.spawn(async move {
                    tokio::time::sleep(Duration::from_secs(seconds)).await;
                    let _ = tx
                        .send_async(SceneManagerMessage::Wake {
                            task_id,
                            result: AsyncTaskResult::Nil,
                        })
                        .await;
                });
                self.threads[task_id]
                    .work
                    .cancel
                    .set_abort(handle.abort_handle());
            }
        }

        Ok(())
    }
    fn handle_yield(
        &mut self,
        lua: &mlua::Lua,
        task_id: TaskId,
        yielded_val: mlua::Value,
    ) -> eyre::Result<()> {
        let t = match yielded_val {
            mlua::Value::Table(t) => t,
            _ => {
                return Err(eyre::eyre!(
                    "Scene yielded an invalid value: expected a yield table, got {}",
                    yielded_val.type_name()
                ));
            }
        };

        let kind = match t.get::<String>("kind") {
            Ok(kind) => kind
                .parse::<YieldKind>()
                .map_err(|err| eyre::eyre!("Scene yielded an invalid kind `{}`: {}", kind, err))?,
            Err(_) => YieldKind::Plugin, // backward compatibility
        };
        match kind {
            YieldKind::Scene => self.handle_scene_yield(lua, task_id, t),
            YieldKind::Plugin => self.handle_plugin_yield(lua, task_id, t),
        }
    }

    pub fn tick(&mut self, lua: &mlua::Lua) {
        self.threads
            .retain(|_, task| task.work.is_alive(lua).unwrap_or(false));
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                SceneManagerMessage::AddTask {
                    thread_rk,
                    label,
                    work,
                } => {
                    let _ = self.add_task(lua, thread_rk, label, work);
                }
                SceneManagerMessage::Cancel(task_id) => {
                    self.threads.remove(task_id);
                }
                SceneManagerMessage::Wake { task_id, result } => {
                    let Some(task) = self.threads.get(task_id) else {
                        continue;
                    };
                    if !task.work.is_alive(lua).unwrap_or(false) {
                        self.threads.remove(task_id);
                        continue;
                    }
                    task.work.cancel.clear_abort();
                    let lua_val = match result {
                        AsyncTaskResult::JsonValue(v) => match json_to_lua(lua, v) {
                            mlua::Result::Ok(v) => v,
                            mlua::Result::Err(err) => {
                                self.log_task_error(
                                    lua,
                                    task_id,
                                    &self.waiting_phase(task_id, "materializing async result"),
                                    &format!("{:#}", err),
                                );
                                self.threads.remove(task_id);
                                continue;
                            }
                        },
                        AsyncTaskResult::String(s) => match lua.create_string(&s) {
                            mlua::Result::Ok(s) => mlua::Value::String(s),
                            mlua::Result::Err(err) => {
                                self.log_task_error(
                                    lua,
                                    task_id,
                                    &self.waiting_phase(task_id, "materializing async result"),
                                    &format!("{:#}", err),
                                );
                                self.threads.remove(task_id);
                                continue;
                            }
                        },
                        AsyncTaskResult::Bool(b) => mlua::Value::Boolean(b),
                        AsyncTaskResult::Nil => mlua::Value::Nil,
                    };

                    if let Err(e) = self.advance_task(lua, task_id, lua_val) {
                        self.log_task_error(
                            lua,
                            task_id,
                            &self.waiting_phase(task_id, "resuming scene"),
                            &format!("{:#}", e),
                        );
                        self.threads.remove(task_id);
                    };
                }
                SceneManagerMessage::Error { task_id, err } => {
                    if !self
                        .threads
                        .get(task_id)
                        .is_some_and(|task| task.work.is_alive(lua).unwrap_or(false))
                    {
                        self.threads.remove(task_id);
                        continue;
                    }
                    self.log_task_error(
                        lua,
                        task_id,
                        &self.waiting_phase(task_id, "running async scene operation"),
                        &err,
                    );
                    self.threads.remove(task_id);
                }
            }
        }
    }
}

impl Drop for SceneTask {
    fn drop(&mut self) {
        self.work.cancel.cancel();
    }
}

#[cfg(test)]
mod tests;
