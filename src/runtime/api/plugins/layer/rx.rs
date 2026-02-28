use mlua::UserData;

use crate::runtime::{api::plugins::layer::handle::LayerHandle, app_data};

struct LayerGuard<'lua> {
    lua: &'lua mlua::Lua,
    is_active: bool,
}
impl<'lua> Drop for LayerGuard<'lua> {
    fn drop(&mut self) {
        if self.is_active {
            if let Some(mut layers) = self.lua.app_data_mut::<app_data::ActiveLayers>() {
                layers.pop();
            }
        }
    }
}

#[derive(Clone)]
pub(super) struct LayerRx {
    callbacks: Vec<mlua::Function>,
    name: Option<String>,
}
impl LayerRx {
    pub fn new() -> Self {
        Self {
            callbacks: Vec::new(),
            name: None,
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
            let mut cleaners: Vec<mlua::Function> = Vec::new();

            let mut is_active = false;
            if let Some(name) = &this.name {
                lua.app_data_mut::<app_data::ActiveLayers>()
                    .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?
                    .push(name.to_owned());
                is_active = true;
            }

            let _guard = LayerGuard { lua, is_active };
            for cb in &this.callbacks {
                let cleaner = cb.call::<Option<mlua::Function>>(())?;
                if let Some(cleaner) = cleaner {
                    match &this.name {
                        Some(name) => {
                            let mut layer_cleaners = lua
                                .app_data_mut::<app_data::LayerCleaners>()
                                .ok_or_else(|| {
                                    mlua::Error::runtime("App data is not initialized")
                                })?;
                            layer_cleaners
                                .entry(name.to_owned())
                                .or_default()
                                .push(cleaner);
                        }
                        None => {
                            cleaners.push(cleaner);
                        }
                    }
                }
            }

            Ok(LayerHandle::new(cleaners))
        });
    }
}
