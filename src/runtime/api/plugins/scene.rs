use color_eyre::eyre;
use mlua::{Function, Lua, Table};

use crate::runtime::{
    api::protocol::{AsyncTask, GameModePlugin, PluginName},
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
        let scene_table = lua.create_table()?;

        let scene_create_fn = lua.create_function(move |lua, table: Table| {
            let name: String = table.get("name").map_err(|_| {
                mlua::Error::runtime(format!(
                    "{}.create: missing `name`. Use `{}.run` for unnamed scenes",
                    plugin_name, plugin_name
                ))
            })?;

            let action: Function = table.get("action").map_err(|_| {
                mlua::Error::runtime(format!("{}.create: missing `action`", plugin_name))
            })?;

            lua.app_data_mut::<app_data::Scenes>()
                .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?
                .insert(name, action);

            Ok(())
        })?;
        scene_table.set("create", scene_create_fn)?;

        let manager_tx = self.manager_tx.clone();
        let scene_run_fn = lua.create_function(move |lua, table: Table| {
            let action: Function = table.get("action").map_err(|_| {
                mlua::Error::runtime(format!("{}.run: missing `action`", plugin_name))
            })?;
            manager_tx
                .send(SceneManagerMessage::AddTask(to_coroutine(lua, action)?))
                .map_err(|e| {
                    mlua::Error::runtime(format!("{}.run: Failed to add task ({})", plugin_name, e))
                })?;
            Ok(())
        })?;
        scene_table.set("run", scene_run_fn)?;

        let manager_tx = self.manager_tx.clone();
        let scene_play_fn = lua.create_function(move |lua, name: String| {
            let scenes = lua
                .app_data_ref::<app_data::Scenes>()
                .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
            let action = scenes.get(&name).ok_or_else(|| {
                mlua::Error::runtime(format!("{}.play: scene {} not found", plugin_name, name))
            })?;

            manager_tx
                .send(SceneManagerMessage::AddTask(to_coroutine(
                    lua,
                    action.clone(),
                )?))
                .map_err(|e| {
                    mlua::Error::runtime(format!("{}.run: Failed to add task ({})", plugin_name, e))
                })?;
            Ok(())
        })?;
        scene_table.set("play", scene_play_fn)?;

        Ok(Some(scene_table))
    }

    fn create_scene_api(&self, _: &Lua) -> mlua::Result<Option<Table>> {
        Ok(None)
    }

    fn handle_op(&self, _: &str, _: Table) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
