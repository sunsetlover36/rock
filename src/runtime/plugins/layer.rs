use color_eyre::eyre;

use crate::runtime::{
    app_data,
    plugins::protocol::{AsyncTask, GameModePlugin, PluginName},
    utils::{get_app_data, get_app_data_mut},
};

mod rx;
use rx::LayerRx;
mod handle;

#[derive(Debug, Clone, Default)]
pub(crate) struct LayerEntry {
    pub name: Option<String>,
    pub cleaners: Vec<mlua::Function>,
}
pub(crate) type LayerId = u64;

fn clear_layer_by_id(lua: &mlua::Lua, id: LayerId) -> mlua::Result<()> {
    let layer = {
        let mut registry = get_app_data_mut::<app_data::LayerRegistry>(lua)?;
        if let Some(layer) = registry.layers.remove(&id) {
            if let Some(name) = &layer.name {
                registry.aliases.remove(name);
            }

            Some(layer)
        } else {
            None
        }
    };

    if let Some(layer) = layer {
        for cleaner in layer.cleaners {
            cleaner.call::<()>(())?;
        }
    }

    Ok(())
}
fn clear_layer_by_name(lua: &mlua::Lua, name: String) -> mlua::Result<()> {
    let id = get_app_data::<app_data::LayerRegistry>(lua)?
        .aliases
        .get(&name)
        .copied();

    if let Some(id) = id {
        clear_layer_by_id(lua, id)?;
    } else {
        eprintln!(
            "Attempt to clear a layer with name {}: layer does not exist",
            name
        );
    }

    Ok(())
}

pub(crate) struct LayerPlugin {}
impl GameModePlugin for LayerPlugin {
    fn name(&self) -> PluginName {
        PluginName::Layer
    }

    fn create_global_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let table = lua.create_table()?;

        let create_fn = lua.create_function(|lua, _: ()| {
            let layer_id = get_app_data_mut::<app_data::LayerRegistry>(lua)?.increment_id();
            Ok(LayerRx::new(layer_id))
        })?;
        table.set("create", create_fn)?;

        let clear_fn = lua.create_function(|lua, name: String| clear_layer_by_name(lua, name))?;
        table.set("clear", clear_fn)?;

        Ok(Some(table))
    }

    fn create_scene_api(&self, _: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(None)
    }
    fn handle_op(&self, _: &mlua::Lua, _: &str, _: mlua::Table) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
