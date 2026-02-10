use std::{str::FromStr, sync::Arc};

use color_eyre::eyre::{self, Context};
use mlua::{Function, Lua, RegistryKey, Table};
use strum::{AsRefStr, Display, EnumString};

use crate::{
    meta_db::MetaDb,
    runtime::{
        api::{
            Yielder,
            protocol::{AsyncTask, AsyncTaskResult, GameModePlugin},
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
    fn name(&self) -> &str {
        "memory"
    }

    fn create_global_api(&self, lua: &Lua) -> eyre::Result<Option<Table>> {
        let memory_global_table = lua
            .create_table()
            .wrap_err("Failed to create `memory_table` for `memory` plugin")?;

        let meta_db = self.meta_db.clone();
        let peek_fn = lua
            .create_function(move |_, key: String| Ok(meta_db.get(key.as_str())))
            .wrap_err("Failed to create `peek` method for `memory` plugin")?;
        memory_global_table
            .set("peek", peek_fn)
            .wrap_err("Failed to register `peek` method for `memory` plugin")?;

        Ok(Some(memory_global_table))
    }

    fn create_scene_api(&self, lua: &Lua) -> eyre::Result<Option<RegistryKey>> {
        let name_in_uppercase = self.name().to_uppercase();
        let memory_scene_table = lua
            .create_table()
            .wrap_err("Failed to create `memory_scene_table` for `memory` plugin")?;

        let global_memory = lua
            .globals()
            .get::<Table>(self.name())
            .wrap_err("Global table `memory` not found")?;
        let mt = lua
            .create_table()
            .wrap_err("Failed to create the metatable for `memory_scene_table`")?;
        mt.set("__index", global_memory)
            .wrap_err("Failed to set `__index` for the metatable of `memory_scene_table`")?;
        memory_scene_table
            .set_metatable(Some(mt))
            .wrap_err("Failed to register the metatable for `memory_scene_table`")?;

        let yielder_fn = Yielder::get(&lua)?;
        let recall_op = format!("{}_{}", &name_in_uppercase, MemoryOp::Recall);
        let recall_fn = yielder_fn.call::<Function>(recall_op).wrap_err(
            "Failed to create `recall` method for `memory_table_async` table using yielder",
        )?;
        memory_scene_table
            .set("recall", recall_fn)
            .wrap_err("Failed to register `recall` method for `memory_scene_table` table")?;

        let fetch_op = format!("{}_{}", &name_in_uppercase, MemoryOp::Fetch);
        let fetch_fn = yielder_fn
            .call::<Function>(fetch_op)
            .wrap_err("Failed to create `fetch` method for `memory_table_async` using yielder")?;
        memory_scene_table
            .set("fetch", fetch_fn)
            .wrap_err("Failed to register `fetch` method for `memory_scene_table` table")?;

        let rk = lua
            .create_registry_value(memory_scene_table)
            .wrap_err("Failed to create `memory_scene_table` registry value")?;
        Ok(Some(rk))
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
