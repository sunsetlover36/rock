use mlua::RegistryKey;
use std::collections::HashMap;

pub struct GameModeAppData {
    pub world_awakes: Option<RegistryKey>,

    pub scenes: HashMap<String, RegistryKey>,
    pub scene_plugins: HashMap<String, RegistryKey>,
    pub yielder: Option<RegistryKey>,
}
