use color_eyre::eyre;
use futures_util::future::BoxFuture;
use mlua::{Lua, Table};
use strum::{AsRefStr, Display, EnumString};

#[derive(Debug)]
pub enum AsyncTaskResult {
    JsonValue(serde_json::Value),
    Nil,
}
pub type AsyncTask = BoxFuture<'static, eyre::Result<AsyncTaskResult>>;

pub trait GameModePlugin {
    fn name(&self) -> PluginName;

    fn create_global_api(&self, lua: &Lua) -> mlua::Result<Option<Table>>;

    fn create_scene_api(&self, lua: &Lua) -> mlua::Result<Option<Table>>;
    fn handle_op(&self, lua: &Lua, op: &str, args: mlua::Value) -> eyre::Result<Option<AsyncTask>>;
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, EnumString, AsRefStr, Display)]
#[strum(serialize_all = "lowercase")]
pub enum PluginName {
    Entity,

    #[strum(serialize = "fc")]
    Farcaster,

    Input,
    Layer,
    Memory,
    On,
    Player,
    Room,
    Scene,
    Timer,

    #[strum(serialize = "Const")]
    Constants,
}
