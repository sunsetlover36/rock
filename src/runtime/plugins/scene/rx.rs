use std::collections::hash_map;

use mlua::UserData;

use super::to_coroutine;
use crate::runtime::{SceneManagerMessage, app_data, utils::get_app_data_mut};

#[derive(Clone)]
pub(super) struct SceneRx {
    manager_tx: flume::Sender<SceneManagerMessage>,
    scripts: Vec<mlua::Function>,
}
impl SceneRx {
    pub fn new(manager_tx: flume::Sender<SceneManagerMessage>) -> Self {
        Self {
            manager_tx,
            scripts: Vec::new(),
        }
    }
}
impl UserData for SceneRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("script", |_, this, script: mlua::Function| {
            let mut next = this.clone();
            next.scripts.push(script);
            Ok(next)
        });

        methods.add_method("play", |lua, this, _: ()| {
            this.manager_tx
                .send(SceneManagerMessage::AddTask(to_coroutine(
                    lua,
                    &this.scripts,
                )?))
                .map_err(|e| {
                    mlua::Error::runtime(format!("scene.play: Failed to add task ({})", e))
                })?;

            Ok(())
        });

        methods.add_method("register", |lua, this, name: String| {
            let mut scenes = get_app_data_mut::<app_data::Scenes>(lua)?;
            match scenes.0.entry(name.clone()) {
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
