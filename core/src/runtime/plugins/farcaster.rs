use color_eyre::eyre;

use super::protocol::{AsyncTask, GameModePlugin, PluginName};

pub(crate) struct FarcasterPlugin {}
impl GameModePlugin for FarcasterPlugin {
    fn name(&self) -> PluginName {
        PluginName::Farcaster
    }

    fn create_global_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let table = lua.create_table()?;

        Ok(Some(table))
    }

    fn create_scene_api(&self, _: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(None)
    }
    fn handle_op(&self, _: &mlua::Lua, _: &str, _: mlua::Table) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
