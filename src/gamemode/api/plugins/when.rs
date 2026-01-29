use color_eyre::eyre;
use mlua::{Lua, RegistryKey, Table};

use crate::gamemode::{
    api::protocol::{AsyncTask, GameModePlugin},
    app_data::GameModeAppData,
    utils::LuaResultExt,
};

pub struct WhenPlugin {}
impl WhenPlugin {
    fn create_world_table(&self, lua: &Lua) -> eyre::Result<Table> {
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
}
impl GameModePlugin for WhenPlugin {
    fn name(&self) -> &str {
        "when"
    }

    fn create_global_api(&self, lua: &Lua) -> eyre::Result<Option<Table>> {
        let when_table = lua
            .create_table()
            .wrap_err("Failed to create `when` namespace")?;
        when_table
            .set("world", self.create_world_table(&lua)?)
            .wrap_err("Failed to register `world` table for `when` namespace")?;

        return Ok(Some(when_table));
    }

    fn create_scene_api(&self, _: &Lua) -> eyre::Result<Option<RegistryKey>> {
        return Ok(None);
    }

    fn handle_op(&self, _: &str, _: Table) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
