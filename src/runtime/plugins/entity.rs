use color_eyre::eyre;

use super::protocol::{AsyncTask, GameModePlugin, PluginName};
use crate::runtime::{app_data, utils::get_app_data_mut};

mod blueprint;
pub(crate) use {blueprint::BlueprintId, blueprint::EntityBlueprint};

pub(crate) mod components;

mod event_descriptors;

mod handle;
pub(crate) use handle::EntityHandle;

mod macros;

mod rx;
use rx::QueryRx;

pub struct EntityPlugin {}
impl GameModePlugin for EntityPlugin {
    fn name(&self) -> PluginName {
        PluginName::Entity
    }

    fn create_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let table = lua.create_table()?;

        let blueprint_fn = lua.create_function(|lua, _: ()| {
            let id = get_app_data_mut::<app_data::BlueprintRegistry>(lua)?.increment_id();
            Ok(EntityBlueprint::new(id))
        })?;
        table.set("blueprint", blueprint_fn)?;

        let query_fn = lua.create_function(|_, _: ()| Ok(QueryRx::default()))?;
        table.set("query", query_fn)?;

        Ok(Some(table))
    }
    fn handle_op(&self, _: &mlua::Lua, _: &str, _: mlua::Value) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
