use color_eyre::eyre;
use mlua::{Lua, Table};

use crate::gamemode::{app_data::GameModeAppData, utils::LuaResultExt};

fn create_world_table(lua: &Lua) -> eyre::Result<Table> {
    let when_world = lua
        .create_table()
        .wrap_err("Failed to create `when_world` table")?;
    let when_world_awakes_fn = lua
        .create_function(|lua, cb: mlua::Function| {
            let rk = lua.create_registry_value(cb)?;

            let mut app_data = lua
                .app_data_mut::<GameModeAppData>()
                .ok_or_else(|| mlua::Error::runtime("GameModeAppData is not initialized"))?;
            app_data.world_awakes = Some(rk);

            Ok(())
        })
        .wrap_err("Failed to create `awakes` method for `when_world` table")?;

    when_world
        .set("awakes", when_world_awakes_fn)
        .wrap_err("Failed to register `awakes` method for `when_world` table")?;

    Ok(when_world)
}

pub fn construct(lua: &Lua) -> eyre::Result<Table> {
    let when_table = lua
        .create_table()
        .wrap_err("Failed to create `when` namespace")?;
    when_table
        .set("world", create_world_table(&lua)?)
        .wrap_err("Failed to register `world` table for `when` namespace")?;

    Ok(when_table)
}
