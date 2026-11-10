use color_eyre::eyre;

use super::protocol::{AsyncTask, GameModePlugin, PluginName};
use crate::runtime::{
    app_data,
    utils::{get_app_data, get_app_data_mut},
};

mod rx;
use rx::LayerRx;
mod handle;

#[derive(Debug, Clone, Default)]
pub(crate) struct LayerEntry {
    pub name: Option<String>,
    pub cleaners: Vec<mlua::Function>,
    pub parent: Option<LayerId>,
    pub children: Vec<LayerId>,
}
pub(crate) type LayerId = u64;

fn clear_layer_by_id(lua: &mlua::Lua, id: LayerId) -> mlua::Result<()> {
    // Removing an initializing ancestor would orphan resources created by the
    // remainder of its callbacks. Rollback calls this only after leaving it.
    if get_app_data::<app_data::ActiveLayers>(lua)?.0.contains(&id) {
        return Err(mlua::Error::runtime(format!(
            "Cannot clear layer {id} while it is initializing"
        )));
    }

    let layer = {
        let mut registry = get_app_data_mut::<app_data::LayerRegistry>(lua)?;
        let Some(layer) = registry.layers.remove(&id) else {
            return Ok(());
        };
        if let Some(name) = &layer.name {
            registry.aliases.remove(name);
        }
        if let Some(parent) = layer.parent {
            if let Some(parent) = registry.layers.get_mut(&parent) {
                parent.children.retain(|child| *child != id);
            }
        }
        layer
    };

    // Detach before invoking Lua so recursive clear calls are harmless. Never
    // hold an app-data borrow across a cleanup callback.
    let mut errors = Vec::new();
    for child in layer.children.into_iter().rev() {
        if let Err(err) = clear_layer_by_id(lua, child) {
            errors.push(err.to_string());
        }
    }
    for cleaner in layer.cleaners.into_iter().rev() {
        if let Err(err) = cleaner.call::<()>(()) {
            errors.push(format!("Layer {id} cleanup failed: {err}"));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(mlua::Error::runtime(errors.join("\n")))
    }
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

    fn create_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
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

    fn handle_op(&self, _: &mlua::Lua, _: &str, _: mlua::Value) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests;
