use std::collections::hash_map;

use mlua::UserData;

use crate::runtime::{
    api::{LayerEntry, LayerId, plugins::layer::handle::LayerHandle},
    app_data,
};

struct LayerGuard<'lua> {
    lua: &'lua mlua::Lua,
}
impl<'lua> Drop for LayerGuard<'lua> {
    fn drop(&mut self) {
        if let Some(mut layers) = self.lua.app_data_mut::<app_data::ActiveLayers>() {
            layers.pop();
        }
    }
}

#[derive(Clone)]
pub(super) struct LayerRx {
    id: LayerId,
    name: Option<String>,
    callbacks: Vec<mlua::Function>,
}
impl LayerRx {
    pub fn new(id: LayerId) -> Self {
        Self {
            id,
            name: None,
            callbacks: Vec::new(),
        }
    }
}
impl UserData for LayerRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("with", |_, this, cb: mlua::Function| {
            let mut next = this.clone();
            next.callbacks.push(cb);

            Ok(next)
        });

        methods.add_method("name", |_, this, name: String| {
            let mut next = this.clone();
            next.name = Some(name);

            Ok(next)
        });

        methods.add_method("commit", |lua, this, _: ()| {
            {
                let mut registry = lua
                    .app_data_mut::<app_data::LayerRegistry>()
                    .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
                if let Some(name) = &this.name {
                    match registry.aliases.entry(name.to_owned()) {
                        hash_map::Entry::Occupied(e) => {
                            if *e.get() != this.id {
                                return Err(mlua::Error::runtime(format!(
                                    "Failed to commit a new layer with the same name {}: layer already exists",
                                    name
                                )));
                            }
                        }
                        hash_map::Entry::Vacant(e) => {
                            e.insert(this.id);
                        }
                    }
                }

                registry.layers.entry(this.id).or_insert(LayerEntry {
                    name: this.name.clone(),
                    cleaners: Vec::new(),
                });
            }

            lua.app_data_mut::<app_data::ActiveLayers>()
                .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?
                .push(this.id);
            let _guard = LayerGuard { lua };

            for cb in &this.callbacks {
                let cleaner = cb.call::<Option<mlua::Function>>(())?;
                if let Some(cleaner) = cleaner {
                    let mut registry = lua
                        .app_data_mut::<app_data::LayerRegistry>()
                        .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
                    registry
                        .layers
                        .entry(this.id)
                        .and_modify(|l| l.cleaners.push(cleaner));
                }
            }

            Ok(LayerHandle::new(this.id))
        });
    }
}
