use color_eyre::eyre;
use strum::IntoEnumIterator;

use crate::runtime::network_replicator::protocol::AreaShape;

use super::protocol::{GameModePlugin, PluginName};

pub(crate) struct ConstantsPlugin {}
impl GameModePlugin for ConstantsPlugin {
    fn name(&self) -> PluginName {
        PluginName::Constants
    }

    fn create_global_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let table = lua.create_table()?;

        let area_shape = lua.create_table()?;
        for shape in AreaShape::iter() {
            let shape = shape.as_ref();
            area_shape.set(shape, shape)?;
        }
        table.set("AreaShape", area_shape)?;

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
