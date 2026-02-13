use color_eyre::eyre;
use mlua::{Lua, Table};

use crate::runtime::{
    api::protocol::{AsyncTask, GameModePlugin},
    utils::LuaResultExt,
};

mod event_descriptors;
use event_descriptors::EVENT_DESCRIPTORS;
mod rx;
use rx::RxBuilder;

pub struct OnPlugin {}
impl GameModePlugin for OnPlugin {
    fn name(&self) -> &str {
        "on"
    }

    fn create_global_api(&self, lua: &Lua) -> eyre::Result<Option<Table>> {
        let on_table = lua
            .create_table()
            .wrap_err(format!("Failed to create `{}` namespace", self.name()).as_str())?;

        for descriptor in EVENT_DESCRIPTORS {
            let ns_table = match on_table.get::<mlua::Table>(descriptor.namespace) {
                Ok(t) => t,
                Err(_) => {
                    let t = lua.create_table().wrap_err(
                        format!("Failed to create `{}` table", descriptor.namespace).as_str(),
                    )?;
                    on_table.set(descriptor.namespace, t.clone()).wrap_err(
                        format!(
                            "Failed to register `{}` table for `{}` namespace",
                            descriptor.namespace,
                            self.name()
                        )
                        .as_str(),
                    )?;
                    t
                }
            };

            let event = descriptor.event;
            let listener = lua
                .create_function(move |_, _: ()| Ok(RxBuilder::new(event)))
                .wrap_err(
                    format!(
                        "Failed to create `{}` method for `{}` table",
                        descriptor.name, descriptor.namespace
                    )
                    .as_str(),
                )?;
            ns_table.set(descriptor.name, listener).wrap_err(
                format!(
                    "Failed to register `{}` method for `{}` table",
                    descriptor.name, descriptor.namespace
                )
                .as_str(),
            )?;
        }

        Ok(Some(on_table))
    }

    fn create_scene_api(&self, _: &Lua) -> eyre::Result<Option<Table>> {
        Ok(None)
    }

    fn handle_op(&self, _: &str, _: Table) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
