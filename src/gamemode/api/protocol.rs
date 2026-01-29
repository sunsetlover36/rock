use color_eyre::eyre::{self};
use futures_util::future::BoxFuture;
use mlua::{Lua, RegistryKey, Table};

use crate::gamemode::api::scheduler::TaskId;

#[derive(Debug)]
pub enum AsyncTaskResult {
    JsonValue(serde_json::Value),
    Text(String),
    Nil,
}
pub type AsyncTask = BoxFuture<'static, eyre::Result<AsyncTaskResult>>;
pub struct AsyncTaskWithId {
    pub id: TaskId,
    pub future: AsyncTask,
}

pub trait GameModePlugin {
    fn name(&self) -> &str;

    fn create_global_api(&self, lua: &Lua) -> eyre::Result<Option<Table>>;
    fn create_scene_api(&self, lua: &Lua) -> eyre::Result<Option<RegistryKey>>;

    fn handle_op(&self, op: &str, args: mlua::Table) -> eyre::Result<Option<AsyncTask>>;
}
