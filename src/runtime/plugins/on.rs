use color_eyre::eyre;

use super::protocol::{AsyncTask, GameModePlugin, PluginName};

pub(crate) mod event_descriptors;
pub(crate) mod lazy;
pub(crate) use lazy::OnPluginLazy;

mod rx;
use rx::OnRx;

pub(crate) mod protocol;
pub(crate) use protocol::GameModeListener;
use protocol::{EventDescriptor, EventScope};

mod handle;

#[derive(Clone)]
pub struct OnPlugin {
    pub descriptors: &'static [EventDescriptor],
}
impl OnPlugin {
    pub fn create_listeners_table(&self, lua: &mlua::Lua) -> mlua::Result<mlua::Table> {
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

            let event_key = descriptor.event_key;
            let listener =
                lua.create_function(move |_, _: ()| Ok(OnRx::new(event_key, EventScope::Global)))?;
            ns_table.set(descriptor.name, listener)?;
        }

        Ok(table)
    }
}
impl GameModePlugin for OnPlugin {
    fn name(&self) -> PluginName {
        PluginName::On
    }

    fn create_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(Some(self.create_listeners_table(lua)?))
    }

    fn handle_op(&self, _: &mlua::Lua, _: &str, _: mlua::Value) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
