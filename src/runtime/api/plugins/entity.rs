use color_eyre::eyre;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::runtime::api::protocol::{GameModePlugin, PluginName};

mod blueprint;
pub(crate) use blueprint::EntityBlueprint;
mod components;
pub(crate) use components::{ComponentData, ComponentKey};
mod event_descriptors;
mod handle;
mod macros;

static BLUEPRINT_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct EntityPlugin {}
impl GameModePlugin for EntityPlugin {
    fn name(&self) -> PluginName {
        PluginName::Entity
    }

    fn create_global_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let entity_table = lua.create_table()?;

        let blueprint_fn = lua.create_function(|_, _: ()| {
            Ok(EntityBlueprint::new(
                BLUEPRINT_COUNTER.fetch_add(1, Ordering::Relaxed),
            ))
        })?;
        entity_table.set("blueprint", blueprint_fn)?;

        Ok(Some(entity_table))
    }
    fn create_scene_api(&self, _: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(None)
    }

    fn handle_op(
        &self,
        _: &str,
        _: mlua::Table,
    ) -> eyre::Result<Option<crate::runtime::api::protocol::AsyncTask>> {
        Ok(None)
    }
}
