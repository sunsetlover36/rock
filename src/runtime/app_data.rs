use std::collections::HashMap;

use crate::runtime::{GameModeEvent, GameModeListener};

pub struct GameModeAppData {
    pub event_listeners: HashMap<GameModeEvent, Vec<GameModeListener>>,
    pub scenes: HashMap<String, mlua::Function>,
    pub scene_plugins: HashMap<String, mlua::Table>,
    pub yielder: Option<mlua::Function>,
}
