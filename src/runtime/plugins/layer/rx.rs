use std::{cell::Cell, rc::Rc};

use mlua::{ErrorContext, UserData};

use super::{LayerEntry, LayerId, clear_layer_by_id, current_layer, handle::LayerHandle};
use crate::runtime::{app_data, utils::get_app_data_mut};

struct LayerGuard<'lua> {
    lua: &'lua mlua::Lua,
}
impl<'lua> Drop for LayerGuard<'lua> {
    fn drop(&mut self) {
        if let Some(mut layers) = self.lua.app_data_mut::<app_data::ActiveLayers>() {
            layers.0.pop();
        }
    }
}

#[derive(Clone)]
pub(super) struct LayerRx {
    id: LayerId,
    name: Option<String>,
    callbacks: Vec<mlua::Function>,
    commit_started: Rc<Cell<bool>>,
}
impl LayerRx {
    pub fn new(id: LayerId) -> Self {
        Self {
            id,
            name: None,
            callbacks: Vec::new(),
            commit_started: Rc::new(Cell::new(false)),
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
            if this.commit_started.get() {
                return Err(mlua::Error::runtime(
                    "Layer builder has already been committed; create a new layer to initialize it again"
                ));
            }

            let parent = current_layer(lua)?;
            {
                let mut registry = get_app_data_mut::<app_data::LayerRegistry>(lua)?;
                if let Some(name) = &this.name {
                    if registry.aliases.contains_key(name) {
                        return Err(mlua::Error::runtime(format!(
                            "Failed to commit a new layer with the same name {name}: layer already exists"
                        )));
                    }
                }
                if let Some(parent) = parent {
                    if !registry.layers.contains_key(&parent) {
                        return Err(mlua::Error::runtime("Parent layer is no longer active"));
                    }
                }

                registry.layers.insert(this.id, LayerEntry {
                    name: this.name.clone(),
                    cleaners: Vec::new(),
                    parent,
                    children: Vec::new(),
                    initializing: true,
                });
                if let Some(name) = &this.name {
                    registry.aliases.insert(name.clone(), this.id);
                }
                if let Some(parent) = parent {
                    registry.layers.get_mut(&parent).unwrap().children.push(this.id);
                }
            }
            // Cloned builders share this flag; reentrant commits are rejected too.
            this.commit_started.set(true);

            let result = (|| -> mlua::Result<()> {
                get_app_data_mut::<app_data::ActiveLayers>(lua)?.0.push(this.id);
                let _guard = LayerGuard { lua };

                for cb in &this.callbacks {
                    if let Some(cleaner) = cb.call::<Option<mlua::Function>>(())? {
                        get_app_data_mut::<app_data::LayerRegistry>(lua)?
                            .layers.get_mut(&this.id).unwrap().cleaners.push(cleaner);
                    }
                }
                Ok(())
            })();

            get_app_data_mut::<app_data::LayerRegistry>(lua)?
                .layers.get_mut(&this.id).unwrap().initializing = false;

            if let Err(err) = result {
                // The initialization guard is gone before rollback starts.
                return match clear_layer_by_id(lua, this.id) {
                    Ok(()) => Err(err),
                    Err(cleanup_err) => Err(err.context(format!(
                        "Layer initialization failed; rollback also reported: {cleanup_err}"
                    ))),
                };
            }

            Ok(LayerHandle::new(this.id))
        });
    }
}
