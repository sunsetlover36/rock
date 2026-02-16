use std::collections::HashMap;

use crate::runtime::api::on::{GameModeEventKey, GameModeListener};

pub struct GameModeAppData {
    pub event_listeners: HashMap<GameModeEventKey, Vec<GameModeListener>>,
    pub scenes: HashMap<String, mlua::Function>,
    pub scene_plugins: HashMap<String, mlua::Table>,
    pub yielder: Option<mlua::Function>,
    pub world: hecs::World,
}
