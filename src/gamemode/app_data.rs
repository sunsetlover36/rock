use mlua::RegistryKey;
use std::collections::HashMap;

pub struct GameModeAppData {
    pub world_awakes: Option<RegistryKey>,
    pub scenes: HashMap<String, RegistryKey>,
    pub memory_table_async: Option<RegistryKey>,
}
