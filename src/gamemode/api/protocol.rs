use color_eyre::eyre;
use mlua::{Lua, RegistryKey, Table};

pub trait GameModePlugin {
    fn name(&self) -> &str;

    fn create_global_api(&self, lua: &Lua) -> eyre::Result<Option<Table>>;
    fn create_scene_api(&self, lua: &Lua) -> eyre::Result<Option<RegistryKey>>;
}
