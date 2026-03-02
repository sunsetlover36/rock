use color_eyre::eyre;

use crate::runtime::{
    api::{
        plugins::entity::rx::EntityRx,
        protocol::{GameModePlugin, PluginName},
    },
    app_data,
    utils::get_app_data_mut,
};

mod blueprint;
pub(crate) use {blueprint::BlueprintId, blueprint::EntityBlueprint};
mod components;
pub(crate) use components::{ComponentData, ComponentKey};
mod event_descriptors;
mod handle;
mod macros;
mod rx;

pub struct EntityPlugin {}
impl GameModePlugin for EntityPlugin {
    fn name(&self) -> PluginName {
        PluginName::Entity
    }

    fn create_global_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let entity_table = lua.create_table()?;

        let blueprint_fn = lua.create_function(|lua, _: ()| {
            let id = get_app_data_mut::<app_data::BlueprintRegistry>(lua)?.increment_id();
            Ok(EntityBlueprint::new(id))
        })?;
        entity_table.set("blueprint", blueprint_fn)?;

        let query_fn = lua.create_function(|_, _: ()| Ok(EntityRx::new()))?;
        entity_table.set("query", query_fn)?;

        Ok(Some(entity_table))
    }
    fn create_scene_api(&self, _: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(None)
    }

    fn handle_op(
        &self,
        _: &mlua::Lua,
        _: &str,
        _: mlua::Table,
    ) -> eyre::Result<Option<crate::runtime::api::protocol::AsyncTask>> {
        Ok(None)
    }
}
