use std::collections::hash_map;

use mlua::UserData;

use crate::runtime::app_data;

#[derive(Clone, Default)]
pub(super) struct SceneRx {
    scripts: Vec<mlua::Function>,
}
impl UserData for SceneRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("script", |_, this, script: mlua::Function| {
            let mut next = this.clone();
            next.scripts.push(script);
            Ok(next)
        });

        methods.add_method("register", |lua, this, name: String| {
            let mut scenes = lua
                .app_data_mut::<app_data::Scenes>()
                .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
            match scenes.entry(name.clone()) {
                hash_map::Entry::Occupied(_) => {
                    return Err(mlua::Error::runtime(format!(
                        "Scene with name {} already exists",
                        name
                    )));
                }
                hash_map::Entry::Vacant(e) => {
                    e.insert(this.scripts.clone());
                }
            }

            Ok(())
        });
    }
}
