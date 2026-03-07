use color_eyre::eyre;
use nanoid::nanoid;

use super::protocol::PluginName;
use crate::runtime::plugins::protocol::GameModePlugin;

pub(crate) struct RoomPlugin {}
impl GameModePlugin for RoomPlugin {
    fn name(&self) -> PluginName {
        PluginName::Room
    }

    fn create_global_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let table = lua.create_table()?;

        let generate_id_fn = lua.create_function(|_, _: ()| Ok(nanoid!()))?;
        table.set("generate_id", generate_id_fn)?;

        let count_fn = lua.create_function(|_, _: ()| {
            // unimpl
            Ok(())
        })?;
        table.set("count", count_fn)?;

        let players_fn = lua.create_function(|_, name: String| {
            // unimpl
            Ok(())
        })?;
        table.set("players", players_fn)?;

        let destroy_fn = lua.create_function(|_, name: String| {
            // unimpl
            Ok(())
        })?;
        table.set("destroy", destroy_fn)?;

        Ok(Some(table))
    }

    fn create_scene_api(&self, _: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(None)
    }
    fn handle_op(
        &self,
        _: &mlua::Lua,
        _: &str,
        _: mlua::Table,
    ) -> eyre::Result<Option<super::protocol::AsyncTask>> {
        Ok(None)
    }
}
