use mlua::RegistryKey;
use std::collections::HashMap;

use crate::runtime::GameModeEvent;

pub struct GameModeAppData {
    pub event_listeners: HashMap<GameModeEvent, Vec<RegistryKey>>,
    pub scenes: HashMap<String, RegistryKey>,
    pub scene_plugins: HashMap<String, RegistryKey>,
    pub yielder: Option<RegistryKey>,
}
