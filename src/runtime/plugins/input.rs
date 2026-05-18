use color_eyre::eyre;
use rock_wire::InputKind;

use super::protocol::{AsyncTask, GameModePlugin, PluginName};

mod rx;
use rx::InputRx;

pub(crate) mod protocol;

pub struct InputPlugin {}
impl GameModePlugin for InputPlugin {
    fn name(&self) -> PluginName {
        PluginName::Input
    }

    fn create_global_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let table = lua.create_table()?;
        table.set(
            "vector",
            lua.create_function(|_, ()| Ok(InputRx::new(InputKind::Vector2D)))?,
        )?;
        table.set(
            "axis",
            lua.create_function(|_, ()| Ok(InputRx::new(InputKind::Axis)))?,
        )?;
        table.set(
            "button",
            lua.create_function(|_, ()| Ok(InputRx::new(InputKind::Button)))?,
        )?;
        Ok(Some(table))
    }

    fn create_scene_api(&self, _: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(None)
    }
    fn handle_op(&self, _: &mlua::Lua, _: &str, _: mlua::Value) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
