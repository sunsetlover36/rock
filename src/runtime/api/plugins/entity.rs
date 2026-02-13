use crate::runtime::{api::protocol::GameModePlugin, utils::LuaResultExt};

mod builder;
use builder::EntityBuilder;
use color_eyre::eyre;
mod components;
pub use components::ComponentVariant;

pub struct EntityPlugin {}
impl GameModePlugin for EntityPlugin {
    fn name(&self) -> &str {
        "entity"
    }

    fn create_global_api(&self, lua: &mlua::Lua) -> eyre::Result<Option<mlua::Table>> {
        let entity_table = lua
            .create_table()
            .wrap_err(format!("Failed to create `{}` table", self.name()).as_str())?;

        let define_fn = lua
            .create_function(|lua, name: mlua::String| Ok(EntityBuilder::new(name)))
            .wrap_err(
                format!(
                    "Failed to create `define` method for `{}` plugin",
                    self.name()
                )
                .as_str(),
            )?;
        entity_table.set("define", define_fn).wrap_err(
            format!(
                "Failed to register `define` method for `{}` plugin",
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
