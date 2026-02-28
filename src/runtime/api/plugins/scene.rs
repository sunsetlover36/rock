use color_eyre::eyre;
use mlua::{Function, Lua, Table};

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

fn get_scene_env(lua: &Lua) -> mlua::Result<Table> {
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
fn to_coroutine(lua: &Lua, function: Function) -> mlua::Result<mlua::RegistryKey> {
    function.set_environment(get_scene_env(&lua)?)?;
    let thread = lua.create_thread(function)?;
    let rk = lua.create_registry_value(thread)?;
    Ok(rk)
}

pub struct ScenePlugin {
    pub manager_tx: flume::Sender<SceneManagerMessage>,
}
impl GameModePlugin for ScenePlugin {
    fn name(&self) -> PluginName {
        PluginName::Scene
    }

    fn create_global_api(&self, lua: &Lua) -> mlua::Result<Option<Table>> {
        let plugin_name = self.name();
        let table = lua.create_table()?;

        let create_fn = lua.create_function(move |_, _: ()| Ok(SceneRx::default()))?;
        table.set("create", create_fn)?;

        let manager_tx = self.manager_tx.clone();
        let scene_run_fn = lua.create_function(move |lua, script: mlua::Function| {
            manager_tx
                .send(SceneManagerMessage::AddTask(to_coroutine(lua, script)?))
                .map_err(|e| {
                    mlua::Error::runtime(format!("{}.run: Failed to add task ({})", plugin_name, e))
                })?;
            Ok(())
        })?;
        table.set("run", scene_run_fn)?;

        let manager_tx = self.manager_tx.clone();
        let scene_play_fn = lua.create_function(move |lua, name: String| {
            let combined_script = lua.create_function(move |lua, _: ()| {
                let scenes = lua
                    .app_data_ref::<app_data::Scenes>()
                    .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
                let scripts = scenes.get(&name).ok_or_else(|| {
                    mlua::Error::runtime(format!("{}.play: scene {} not found", plugin_name, name))
                })?;

                for script in scripts {
                    script.call::<()>(())?;
                }

                Ok(())
            })?;

            manager_tx
                .send(SceneManagerMessage::AddTask(to_coroutine(
                    lua,
                    combined_script,
                )?))
                .map_err(|e| {
                    mlua::Error::runtime(format!("{}.run: Failed to add task ({})", plugin_name, e))
                })?;
            Ok(())
        })?;
        table.set("play", scene_play_fn)?;

        Ok(Some(table))
    }

    fn create_scene_api(&self, _: &Lua) -> mlua::Result<Option<Table>> {
        Ok(None)
    }

    fn handle_op(&self, _: &str, _: Table) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
