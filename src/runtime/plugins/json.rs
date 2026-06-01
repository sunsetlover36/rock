use color_eyre::eyre;
use mlua::LuaSerdeExt;

use crate::{runtime::plugins::protocol::GameModePlugin, utils::json_to_lua};

use super::protocol::PluginName;

pub(crate) struct JsonPlugin {}
impl GameModePlugin for JsonPlugin {
    fn name(&self) -> PluginName {
        PluginName::Json
    }

    fn create_global_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let table = lua.create_table()?;

        table.set(
            "stringify",
            lua.create_function(|lua, value: mlua::Value| {
                let value: serde_json::Value = lua.from_value(value)?;
                serde_json::to_string(&value).map_err(mlua::Error::runtime)
            })?,
        )?;

        table.set(
            "parse",
            lua.create_function(|lua, s: String| {
                let value: serde_json::Value =
                    serde_json::from_str(&s).map_err(mlua::Error::runtime)?;
                json_to_lua(lua, value)
            })?,
        )?;

        table.set(
            "array",
            lua.create_function(|lua, values: mlua::Variadic<mlua::Value>| {
                let table = lua.create_table()?;

                for (i, value) in values.into_iter().enumerate() {
                    table.set(i + 1, value)?;
                }

                table.set_metatable(Some(lua.array_metatable()))?;
                Ok(table)
            })?,
        )?;

        table.set(
            "as_array",
            lua.create_function(|lua, table: mlua::Table| {
                table.set_metatable(Some(lua.array_metatable()))?;
                Ok(table)
            })?,
        )?;

        Ok(Some(table))
    }

    fn create_scene_api(&self, _: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(None)
    }
    fn handle_op(
        &self,
        _: &mlua::Lua,
        _: &str,
        _: mlua::Value,
    ) -> eyre::Result<Option<super::protocol::AsyncTask>> {
        Ok(None)
    }
}
