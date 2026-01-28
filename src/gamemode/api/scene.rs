use color_eyre::eyre;
use mlua::{Function, Lua, RegistryKey, Table};

use crate::gamemode::{
    api::protocol::GameModePlugin, app_data::GameModeAppData, utils::LuaResultExt,
};

fn get_scene_env(lua: &Lua) -> mlua::Result<Table> {
    let app_data = lua
        .app_data_ref::<GameModeAppData>()
        .ok_or_else(|| mlua::Error::runtime("GameModeAppData is not initialized"))?;

    let env = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.set("__index", lua.globals())?;
    env.set_metatable(Some(mt))?;

    for plugin in app_data.scene_plugins.iter() {
        let (name, rk) = plugin;
        let table: Table = lua.registry_value(&rk)?;
        env.set(name.to_owned(), table)?;
    }

    Ok(env)
}
fn to_coroutine(lua: &Lua, function: Function) -> mlua::Result<mlua::Thread> {
    function.set_environment(get_scene_env(&lua)?)?;

    Ok(lua.create_thread(function)?)
}

pub struct ScenePlugin {}
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
                let rk = lua.create_registry_value(action)?;

                let mut app_data = lua
                    .app_data_mut::<GameModeAppData>()
                    .ok_or_else(|| mlua::Error::runtime("GameModeAppData is not initialized"))?;
                app_data.scenes.insert(name, rk);

                Ok(())
            })
            .wrap_err("Failed to create `create` method for `scene` table")?;
        scene_table
            .set("create", scene_create_fn)
            .wrap_err("Failed to register `create` method for `scene` table")?;

        let scene_run_fn = lua
            .create_function(|lua, table: Table| {
                let action: Function = table
                    .get("action")
                    .map_err(|_| mlua::Error::runtime("scene.run: missing `action`"))?;
                let coroutine = to_coroutine(lua, action)?;
                Ok(())
            })
            .wrap_err("Failed to create `run` method for `scene` table")?;
        scene_table
            .set("run", scene_run_fn)
            .wrap_err("Failed to register `run` method for `scene` table")?;

        let scene_play_fn = lua
            .create_function(|lua, name: String| {
                let app_data = lua
                    .app_data_ref::<GameModeAppData>()
                    .ok_or_else(|| mlua::Error::runtime("GameModeAppData is not initialized"))?;
                let rk = app_data.scenes.get(&name).ok_or_else(|| {
                    mlua::Error::runtime(format!("scene.play: scene {} not found", name))
                })?;

                let action: Function = lua.registry_value(rk)?;
                let coroutine = to_coroutine(lua, action)?;
                Ok(())
            })
            .wrap_err("Failed to create `play` method for `scene` table")?;
        scene_table
            .set("play", scene_play_fn)
            .wrap_err("Failed to register `play` method for `scene` table")?;

        Ok(Some(scene_table))
    }

    fn create_scene_api(&self, _: &Lua) -> eyre::Result<Option<RegistryKey>> {
        Ok(None)
    }
}
