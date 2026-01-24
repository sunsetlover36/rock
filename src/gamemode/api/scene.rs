use color_eyre::eyre;
use mlua::{Function, Lua, Table};

use crate::gamemode::{app_data::GameModeAppData, utils::LuaResultExt};

pub fn register(lua: &Lua) -> eyre::Result<()> {
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

    lua.globals()
        .set("scene", scene_table)
        .wrap_err("Failed to register `scene` table")?;
    Ok(())
}
