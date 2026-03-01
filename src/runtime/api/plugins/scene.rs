use color_eyre::eyre;
use mlua::Lua;

use crate::runtime::{
    api::{
        plugins::scene::rx::SceneRx,
        protocol::{AsyncTask, GameModePlugin, PluginName},
    },
    app_data,
};

mod manager;
pub use manager::{SceneManager, SceneManagerMessage, SceneManagerParams};
mod rx;

fn get_scene_env(lua: &Lua) -> mlua::Result<mlua::Table> {
    let env = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.set("__index", lua.globals())?;
    env.set_metatable(Some(mt))?;

    let scene_plugins = lua
        .app_data_ref::<app_data::ScenePlugins>()
        .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
    for plugin in scene_plugins.iter() {
        let (name, table) = plugin;
        env.set(name.as_ref(), table)?;
    }

    Ok(env)
}
fn to_coroutine(lua: &Lua, functions: &Vec<mlua::Function>) -> mlua::Result<mlua::RegistryKey> {
    let env = get_scene_env(lua)?;
    for function in functions {
        function.set_environment(env.clone())?;
    }

    let iter: mlua::Function = lua
        .load(
            r#"
            return function(funcs)
                for i = 1, #funcs do
                    funcs[i]()
                end
            end
            "#,
        )
        .eval()?;
    let coroutine = iter.bind(lua.create_sequence_from(functions)?)?;
    let rk = lua.create_registry_value(lua.create_thread(coroutine)?)?;
    Ok(rk)
}

pub struct ScenePlugin {
    pub manager_tx: flume::Sender<SceneManagerMessage>,
}
impl GameModePlugin for ScenePlugin {
    fn name(&self) -> PluginName {
        PluginName::Scene
    }

    fn create_global_api(&self, lua: &Lua) -> mlua::Result<Option<mlua::Table>> {
        let plugin_name = self.name();
        let table = lua.create_table()?;

        let manager_tx = self.manager_tx.clone();
        let create_fn =
            lua.create_function(move |_, _: ()| Ok(SceneRx::new(manager_tx.clone())))?;
        table.set("create", create_fn)?;

        let manager_tx = self.manager_tx.clone();
        let scene_run_fn = lua.create_function(move |lua, script: mlua::Function| {
            manager_tx
                .clone()
                .send(SceneManagerMessage::AddTask(to_coroutine(
                    lua,
                    &vec![script],
                )?))
                .map_err(|e| {
                    mlua::Error::runtime(format!("{}.run: Failed to add task ({})", plugin_name, e))
                })?;
            Ok(())
        })?;
        table.set("run", scene_run_fn)?;

        let manager_tx = self.manager_tx.clone();
        let scene_play_fn = lua.create_function(move |lua, name: String| {
            let coroutine = {
                let scenes = lua
                    .app_data_ref::<app_data::Scenes>()
                    .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
                let scripts = scenes.get(&name).ok_or_else(|| {
                    mlua::Error::runtime(format!("{}.play: scene {} not found", plugin_name, name))
                })?;

                to_coroutine(lua, scripts)?
            };

            manager_tx
                .send(SceneManagerMessage::AddTask(coroutine))
                .map_err(|e| {
                    mlua::Error::runtime(format!("{}.run: Failed to add task ({})", plugin_name, e))
                })?;
            Ok(())
        })?;
        table.set("play", scene_play_fn)?;

        Ok(Some(table))
    }

    fn create_scene_api(&self, _: &Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(None)
    }

    fn handle_op(&self, _: &str, _: mlua::Table) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
