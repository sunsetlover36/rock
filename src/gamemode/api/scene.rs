use color_eyre::eyre;
use mlua::{Function, Lua, Table};

use crate::gamemode::{app_data::GameModeAppData, utils::LuaResultExt};

fn get_scene_env(lua: &Lua) -> mlua::Result<Table> {
    let app_data = lua
        .app_data_ref::<GameModeAppData>()
        .ok_or_else(|| mlua::Error::runtime("GameModeAppData is not initialized"))?;
    let memory_table_async_rk = app_data.memory_table_async.as_ref().ok_or_else(|| {
        mlua::Error::runtime("Registry key for `memory_table_async` table not found")
    })?;
    let memory_async_table = lua.registry_value::<Table>(memory_table_async_rk)?;

    let env = lua.create_table()?;
    env.set("memory", memory_async_table)?;

    let mt = lua.create_table()?;
    mt.set("__index", lua.globals())?;
    env.set_metatable(Some(mt))?;

    Ok(env)
}
pub fn construct(lua: &Lua) -> eyre::Result<Table> {
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
            action.set_environment(get_scene_env(lua)?)?;
            action.call::<()>(())?;

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
            action.set_environment(get_scene_env(lua)?)?;
            action.call::<()>(())?;

            Ok(())
        })
        .wrap_err("Failed to create `play` method for `scene` table")?;
    scene_table
        .set("play", scene_play_fn)
        .wrap_err("Failed to register `play` method for `scene` table")?;

    Ok(scene_table)
}
