use std::sync::atomic::{AtomicU64, Ordering};

use crate::runtime::{api::protocol::GameModePlugin, utils::LuaResultExt};
use color_eyre::eyre;

mod blueprint;
use blueprint::EntityBlueprint;
mod components;
pub(crate) use components::{ComponentData, ComponentKey};
mod event_descriptors;
mod handle;
mod macros;

static BLUEPRINT_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct EntityPlugin {}
impl GameModePlugin for EntityPlugin {
    fn name(&self) -> &str {
        "entity"
    }

    fn create_global_api(&self, lua: &mlua::Lua) -> eyre::Result<Option<mlua::Table>> {
        let entity_table = lua
            .create_table()
            .wrap_err(format!("Failed to create `{}` table", self.name()).as_str())?;

        let blueprint_fn = lua
            .create_function(|_, _: ()| {
                Ok(EntityBlueprint::new(
                    BLUEPRINT_COUNTER.fetch_add(1, Ordering::Relaxed),
                ))
            })
            .wrap_err(
                format!(
                    "Failed to create `blueprint` method for `{}` plugin",
                    self.name()
                )
                .as_str(),
            )?;
        entity_table.set("blueprint", blueprint_fn).wrap_err(
            format!(
                "Failed to register `blueprint` method for `{}` plugin",
                self.name()
            )
            .as_str(),
        )?;

        Ok(Some(entity_table))
    }
    fn create_scene_api(&self, _: &mlua::Lua) -> color_eyre::eyre::Result<Option<mlua::Table>> {
        Ok(None)
    }

    fn handle_op(
        &self,
        _: &str,
        _: mlua::Table,
    ) -> color_eyre::eyre::Result<Option<crate::runtime::api::protocol::AsyncTask>> {
        Ok(None)
    }
}
