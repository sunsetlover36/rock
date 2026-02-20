use std::{str::FromStr, sync::Arc};

use color_eyre::eyre::{self, Context};
use mlua::{Function, Lua, Table};
use strum::{AsRefStr, Display, EnumString};

use crate::{
    meta_db::MetaDb,
    runtime::{
        api::{
            Yielder,
            protocol::{AsyncTask, AsyncTaskResult, GameModePlugin, PluginName},
        },
        utils::LuaResultExt,
    },
};

#[derive(Debug, Clone, Copy, EnumString, Display, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum MemoryOp {
    Fetch,
    Recall,
}

pub struct MemoryPlugin {
    pub meta_db: Arc<MetaDb>,
}
impl GameModePlugin for MemoryPlugin {
    fn name(&self) -> PluginName {
        PluginName::Memory
    }

    fn create_global_api(&self, lua: &Lua) -> mlua::Result<Option<Table>> {
        let table = lua.create_table()?;

        let meta_db = self.meta_db.clone();
        let peek_fn = lua.create_function(move |_, key: String| Ok(meta_db.get(key.as_str())))?;
        table.set("peek", peek_fn)?;

        Ok(Some(table))
    }

    fn create_scene_api(&self, lua: &Lua) -> mlua::Result<Option<Table>> {
        let plugin_name = self.name().to_string();
        let name_in_uppercase = plugin_name.to_uppercase();
        let table = lua.create_table()?;

        let global_memory = lua.globals().get::<Table>(plugin_name)?;
        let mt = lua.create_table()?;
        mt.set("__index", global_memory)?;
        table.set_metatable(Some(mt))?;

        let yielder_fn = Yielder::get(&lua)?;
        let recall_op = format!("{}_{}", &name_in_uppercase, MemoryOp::Recall);
        let recall_fn = yielder_fn.call::<Function>(recall_op)?;
        table.set("recall", recall_fn)?;

        let fetch_op = format!("{}_{}", &name_in_uppercase, MemoryOp::Fetch);
        let fetch_fn = yielder_fn.call::<Function>(fetch_op)?;
        table.set("fetch", fetch_fn)?;

        Ok(Some(table))
    }

    fn handle_op(&self, op: &str, args: mlua::Table) -> eyre::Result<Option<AsyncTask>> {
        let meta_db = self.meta_db.clone();

        let op =
            MemoryOp::from_str(op).wrap_err_with(|| format!("Unknown memory plugin op: {}", op))?;
        match op {
            MemoryOp::Recall => {
                let key: String = args
                    .get(1)
                    .wrap_err("Missing argument for `memory.recall` method")?;
                let future = Box::pin(async move {
                    let res = if key.ends_with("/") {
                        meta_db.get_or_ensure_prefix(&key).await?
                    } else {
                        meta_db.get_or_ensure_key(&key).await?
                    };

                    Ok(match res {
                        Some(v) => AsyncTaskResult::JsonValue(v),
                        None => AsyncTaskResult::Nil,
                    })
                });

                Ok(Some(future))
            }
            MemoryOp::Fetch => {
                let key: String = args
                    .get(1)
                    .wrap_err("Missing argument for `memory.fetch` method")?;
                let future = Box::pin(async move {
                    let res = if key.ends_with("/") {
                        meta_db.ensure_prefix(&key).await?
                    } else {
                        meta_db.ensure_key(&key).await?
                    };

                    Ok(match res {
                        Some(v) => AsyncTaskResult::JsonValue(v),
                        None => AsyncTaskResult::Nil,
                    })
                });

                Ok(Some(future))
            }
        }
    }
}
