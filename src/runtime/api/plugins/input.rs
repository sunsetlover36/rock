use color_eyre::eyre;

use crate::runtime::api::protocol::{AsyncTask, GameModePlugin, PluginName};

mod rx;
use rx::InputRxBuilder;
mod protocol;

pub struct InputPlugin {}
impl GameModePlugin for InputPlugin {
    fn name(&self) -> PluginName {
        PluginName::Input
    }

    fn create_global_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let plugin_name = self.name();

        let table = lua.create_table()?;
        table.set(
            "new",
            lua.create_function(|_, ()| Ok(InputRxBuilder::new()))?,
        )?;
        Ok(Some(table))
    }

    fn create_scene_api(&self, _: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(None)
    }
    fn handle_op(&self, _: &str, _: mlua::Table) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
