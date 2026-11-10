use color_eyre::eyre;
use mlua::Lua;

use super::protocol::{AsyncTask, GameModePlugin, PluginName};
use crate::runtime::{app_data, utils::get_app_data};

mod manager;
pub(crate) use manager::{SceneManager, SceneManagerParams};

mod rx;
use rx::SceneRx;

mod ctx;
mod protocol;
pub(crate) use protocol::SceneManagerMessage;

fn get_scene_env(lua: &Lua) -> mlua::Result<mlua::Table> {
    let env = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.set("__index", lua.globals())?;
    env.set_metatable(Some(mt))?;
    Ok(env)
}
fn function_location(function: &mlua::Function) -> String {
    let info = function.info();
    let source = info
        .short_src
        .or(info.source)
        .unwrap_or_else(|| "<unknown>".to_owned());

    match (info.name, info.line_defined) {
        (Some(name), Some(line)) => format!("{name} at {source}:{line}"),
        (Some(name), None) => format!("{name} at {source}"),
        (None, Some(line)) => format!("{source}:{line}"),
        (None, None) => source,
    }
}

pub(super) fn script_chain_label(functions: &[mlua::Function]) -> String {
    match functions {
        [] => "empty script chain".to_owned(),
        [function] => function_location(function),
        functions => {
            let locations = functions
                .iter()
                .map(function_location)
                .collect::<Vec<_>>()
                .join(" -> ");
            format!("{} scripts: {}", functions.len(), locations)
        }
    }
}

fn to_coroutine(lua: &Lua, functions: &[mlua::Function]) -> mlua::Result<mlua::RegistryKey> {
    let env = get_scene_env(lua)?;
    for function in functions {
        function.set_environment(env.clone())?;
    }
    let iter: mlua::Function = lua
        .load(
            r#"
            return function(funcs, ctx)
                for i = 1, #funcs do
                    funcs[i](ctx)
                end
            end
            "#,
        )
        .eval()?;
    let coroutine = iter.bind(lua.create_sequence_from(functions)?)?;
    let rk = lua.create_registry_value(lua.create_thread(coroutine)?)?;
    Ok(rk)
}

#[derive(Clone)]
pub(crate) struct SceneDefinition {
    pub scripts: Vec<mlua::Function>,
    pub players: Vec<rock_wire::PlayerKey>,
}

pub(super) struct SceneHandle {
    cancel: crate::runtime::lifetime::Cancellation,
}
impl mlua::UserData for SceneHandle {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("cancel", |_, this, ()| {
            this.cancel.cancel();
            Ok(())
        });
    }
}

fn launch(
    lua: &Lua,
    tx: &flume::Sender<SceneManagerMessage>,
    scripts: &[mlua::Function],
    players: &[rock_wire::PlayerKey],
    label: String,
) -> mlua::Result<SceneHandle> {
    let context = crate::runtime::lifetime::WorkContext::capture(lua, players)?;
    let thread_rk = to_coroutine(lua, scripts)?;
    let work = crate::runtime::lifetime::OwnedWork::register(lua, context)?;
    let cancel = work.cancel.clone();
    tx.try_send(SceneManagerMessage::AddTask {
        thread_rk,
        label,
        work,
    })
    .map_err(|e| {
        cancel.cancel();
        mlua::Error::runtime(format!("scene: failed to queue task: {e}"))
    })?;
    Ok(SceneHandle { cancel })
}

pub struct ScenePlugin {
    pub manager_tx: flume::Sender<SceneManagerMessage>,
}
impl GameModePlugin for ScenePlugin {
    fn name(&self) -> PluginName {
        PluginName::Scene
    }

    fn create_api(&self, lua: &Lua) -> mlua::Result<Option<mlua::Table>> {
        let table = lua.create_table()?;

        let manager_tx = self.manager_tx.clone();
        let create_fn =
            lua.create_function(move |_, _: ()| Ok(SceneRx::new(manager_tx.clone())))?;
        table.set("create", create_fn)?;

        let manager_tx = self.manager_tx.clone();
        table.set(
            "run",
            lua.create_function(move |lua, script: mlua::Function| {
                let scripts = vec![script];
                let label = format!("scene.run ({})", script_chain_label(&scripts));
                launch(lua, &manager_tx, &scripts, &[], label)
            })?,
        )?;

        let manager_tx = self.manager_tx.clone();
        table.set(
            "play",
            lua.create_function(move |lua, name: String| {
                let definition = get_app_data::<app_data::Scenes>(lua)?
                    .0
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| {
                        mlua::Error::runtime(format!("scene.play: scene {name} not found"))
                    })?;
                let label = format!(
                    "scene.play({name:?}) ({})",
                    script_chain_label(&definition.scripts)
                );
                launch(
                    lua,
                    &manager_tx,
                    &definition.scripts,
                    &definition.players,
                    label,
                )
            })?,
        )?;

        Ok(Some(table))
    }

    fn handle_op(&self, _: &Lua, _: &str, _: mlua::Value) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
