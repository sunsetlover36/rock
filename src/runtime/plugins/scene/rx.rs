use std::collections::hash_map;

use mlua::UserData;

use super::{SceneDefinition, launch, script_chain_label};
use crate::runtime::{SceneManagerMessage, app_data, utils::get_app_data_mut};

#[derive(Clone)]
pub(super) struct SceneRx {
    manager_tx: flume::Sender<SceneManagerMessage>,
    scripts: Vec<mlua::Function>,
    players: Vec<rock_wire::PlayerKey>,
}
impl SceneRx {
    pub fn new(manager_tx: flume::Sender<SceneManagerMessage>) -> Self {
        Self {
            manager_tx,
            scripts: Vec::new(),
            players: Vec::new(),
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
            let label = format!(
                "scene.create(...).play ({})",
                script_chain_label(&this.scripts)
            );
            launch(lua, &this.manager_tx, &this.scripts, &this.players, label)
        });

        methods.add_method(
            "while_connected",
            |_, this, p: mlua::UserDataRef<crate::runtime::plugins::player::PlayerHandle>| {
                let mut next = this.clone();
                if !next.players.contains(&p.key()) {
                    next.players.push(p.key());
                }
                Ok(next)
            },
        );

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
                    e.insert(SceneDefinition {
                        scripts: this.scripts.clone(),
                        players: this.players.clone(),
                    });
                }
            }

            Ok(())
        });
    }
}
