use color_eyre::eyre;
use mlua::{Function, Lua, Table};

use crate::runtime::{
    api::protocol::{AsyncTask, GameModePlugin},
    app_data::GameModeAppData,
    utils::LuaResultExt,
};

mod manager;
pub use manager::{SceneManager, SceneManagerMessage, SceneManagerParams};

fn get_scene_env(lua: &Lua) -> mlua::Result<Table> {
    let app_data = lua
        .app_data_ref::<GameModeAppData>()
        .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;

    let env = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.set("__index", lua.globals())?;
    env.set_metatable(Some(mt))?;

    for plugin in app_data.scene_plugins.iter() {
        let (name, table) = plugin;
        env.set(name.to_owned(), table)?;
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
    fn name(&self) -> &str {
        "scene"
    }

    fn create_global_api(&self, lua: &Lua) -> eyre::Result<Option<Table>> {
        let scene_table = lua
            .create_table()
            .wrap_err("Failed to create namespace `scene`")?;

        let scene_create_fn = lua
            .create_function(|lua, table: Table| {
                let name: String = table.get("name").map_err(|_| {
                    mlua::Error::runtime(
                        "scene.create: missing `name`. Use `scene.run` for unnamed scenes",
                    )
                })?;

                let action: Function = table
                    .get("action")
                    .map_err(|_| mlua::Error::runtime("scene.create: missing `action`"))?;

                let mut app_data = lua
                    .app_data_mut::<GameModeAppData>()
                    .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
                app_data.scenes.insert(name, action);

                Ok(())
            })
            .wrap_err("Failed to create `create` method for `scene` table")?;
        scene_table
            .set("create", scene_create_fn)
            .wrap_err("Failed to register `create` method for `scene` table")?;

        let manager_tx = self.manager_tx.clone();
        let scene_run_fn = lua
            .create_function(move |lua, table: Table| {
                let action: Function = table
                    .get("action")
                    .map_err(|_| mlua::Error::runtime("scene.run: missing `action`"))?;
                manager_tx
                    .send(SceneManagerMessage::AddTask(to_coroutine(lua, action)?))
                    .map_err(|e| {
                        mlua::Error::runtime(format!("scene.run: Failed to add task ({})", e))
                    })?;
                Ok(())
            })
            .wrap_err("Failed to create `run` method for `scene` table")?;
        scene_table
            .set("run", scene_run_fn)
            .wrap_err("Failed to register `run` method for `scene` table")?;

        let manager_tx = self.manager_tx.clone();
        let scene_play_fn = lua
            .create_function(move |lua, name: String| {
                let app_data = lua
                    .app_data_ref::<GameModeAppData>()
                    .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
                let action = app_data.scenes.get(&name).ok_or_else(|| {
                    mlua::Error::runtime(format!("scene.play: scene {} not found", name))
                })?;

                manager_tx
                    .send(SceneManagerMessage::AddTask(to_coroutine(
                        lua,
                        action.clone(),
                    )?))
                    .map_err(|e| {
                        mlua::Error::runtime(format!("scene.run: Failed to add task ({})", e))
                    })?;
                Ok(())
            })
            .wrap_err("Failed to create `play` method for `scene` table")?;
        scene_table
            .set("play", scene_play_fn)
            .wrap_err("Failed to register `play` method for `scene` table")?;

        Ok(Some(scene_table))
    }

    fn create_scene_api(&self, _: &Lua) -> eyre::Result<Option<Table>> {
        Ok(None)
    }

    fn handle_op(&self, _: &str, _: Table) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
