use color_eyre::eyre;
use mlua::{Lua, Table};

use crate::runtime::{
    api::protocol::{AsyncTask, GameModePlugin},
    utils::LuaResultExt,
};

pub(crate) mod event_descriptors;
mod rx;
use rx::RxBuilder;
pub mod protocol;
pub use protocol::*;

#[derive(Clone)]
pub struct OnPlugin {
    pub descriptors: &'static [EventDescriptor],
}
impl OnPlugin {
    pub fn create_listeners_table(
        &self,
        lua: &Lua,
        scope: Option<EventScope>,
    ) -> mlua::Result<Table> {
        println!(
            "Request to create a new listeners table with scope: {:?}",
            scope
        );
        let table = lua.create_table()?;

        for descriptor in self.descriptors {
            let ns_table = match descriptor.namespace {
                Some(ns) => match table.get::<mlua::Table>(ns) {
                    Ok(t) => t,
                    Err(_) => {
                        let t = lua.create_table()?;
                        table.set(descriptor.namespace, t.clone())?;
                        t
                    }
                },
                None => table.clone(),
            };

            let key = descriptor.event_key;
            let listener = lua.create_function(move |_, _: ()| Ok(RxBuilder::new(key, scope)))?;
            ns_table.set(descriptor.name, listener)?;
        }

        Ok(table)
    }
}
impl GameModePlugin for OnPlugin {
    fn name(&self) -> &str {
        "on"
    }

    fn create_global_api(&self, lua: &Lua) -> eyre::Result<Option<Table>> {
        Ok(Some(self.create_listeners_table(lua, None).wrap_err(
            format!("Failed to initialize `{}` plugin", self.name()).as_str(),
        )?))
    }

    fn create_scene_api(&self, _: &Lua) -> eyre::Result<Option<Table>> {
        Ok(None)
    }

    fn handle_op(&self, _: &str, _: Table) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
