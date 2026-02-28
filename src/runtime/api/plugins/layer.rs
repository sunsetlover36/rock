use color_eyre::eyre;

use crate::runtime::api::protocol::{AsyncTask, GameModePlugin, PluginName};

mod rx;
use rx::LayerRx;
mod handle;

pub(crate) struct LayerPlugin {}
impl GameModePlugin for LayerPlugin {
    fn name(&self) -> PluginName {
        PluginName::Layer
    }

    fn create_global_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let table = lua.create_table()?;

        let create_fn = lua.create_function(|_, _: ()| Ok(LayerRx::new()))?;
        table.set("create", create_fn)?;

        Ok(Some(table))
    }

    fn create_scene_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(None)
    }
    fn handle_op(&self, op: &str, args: mlua::Table) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
