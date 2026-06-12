use std::{str::FromStr, sync::Arc};

use color_eyre::eyre::{self, Context};
use mlua::{Lua, LuaSerdeExt, Table};
use strum::{AsRefStr, Display, EnumString};

use super::{
    protocol::{AsyncTask, AsyncTaskResult, GameModePlugin, PluginName},
    yield_plugin_op,
};
use crate::{
    meta_db::MetaDb,
    runtime::{
        app_data, get_app_data, network_replicator::protocol::ReplicationMark, utils::LuaResultExt,
    },
};

mod rx;
use rx::SyncRx;

#[derive(Debug, Clone, Copy, EnumString, Display, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum MemoryOp {
    Fetch,
    Recall,
    Store,
    Delete,
}

pub(crate) struct MemoryPlugin {
    pub meta_db: Arc<MetaDb>,
}
impl GameModePlugin for MemoryPlugin {
    fn name(&self) -> PluginName {
        PluginName::Memory
    }

    fn create_api(&self, lua: &Lua) -> mlua::Result<Option<Table>> {
        let plugin_name = self.name();
        let name_in_uppercase = plugin_name.to_string().to_uppercase();
        let table = lua.create_table()?;

        let meta_db = self.meta_db.clone();
        let peek_fn = lua.create_function(move |_, key: String| {
            let v = meta_db.get(key.as_str()).map_err(|e| {
                let report = eyre::ErrReport::from(e).wrap_err(format!(
                    "{}.peek: failed to get a value from key '{}'",
                    plugin_name, key
                ));

                mlua::Error::external(format!("{:#}", report))
            })?;

            Ok(v)
        })?;
        table.set("peek", peek_fn)?;

        let node_fn = lua.create_function(|_, key: String| Ok(SyncRx::new(key)))?;
        table.set("node", node_fn)?;

        let recall_op = format!("{}_{}", &name_in_uppercase, MemoryOp::Recall);
        let recall_fn = lua.create_async_function(move |lua, key: String| {
            let opcode = recall_op.clone();
            async move {
                let args = lua.create_sequence_from([key])?;
                yield_plugin_op(&lua, "memory.recall", opcode, mlua::Value::Table(args)).await
            }
        })?;
        table.set("recall", recall_fn)?;

        let fetch_op = format!("{}_{}", &name_in_uppercase, MemoryOp::Fetch);
        let fetch_fn = lua.create_async_function(move |lua, key: String| {
            let opcode = fetch_op.clone();
            async move {
                let args = lua.create_sequence_from([key])?;
                yield_plugin_op(&lua, "memory.fetch", opcode, mlua::Value::Table(args)).await
            }
        })?;
        table.set("fetch", fetch_fn)?;

        let store_op = format!("{}_{}", &name_in_uppercase, MemoryOp::Store);
        let store_fn =
            lua.create_async_function(move |lua, (key, value): (String, mlua::Value)| {
                let opcode = store_op.clone();
                async move {
                    let args = lua.create_table()?;
                    args.set(1, key)?;
                    args.set(2, value)?;
                    yield_plugin_op(&lua, "memory.store", opcode, mlua::Value::Table(args)).await
                }
            })?;
        table.set("store", store_fn)?;

        let delete_op = format!("{}_{}", &name_in_uppercase, MemoryOp::Delete);
        let delete_fn = lua.create_async_function(move |lua, key: String| {
            let opcode = delete_op.clone();
            async move {
                let args = lua.create_sequence_from([key])?;
                yield_plugin_op(&lua, "memory.delete", opcode, mlua::Value::Table(args)).await
            }
        })?;
        table.set("delete", delete_fn)?;

        Ok(Some(table))
    }
    fn handle_op(
        &self,
        lua: &mlua::Lua,
        op: &str,
        args: mlua::Value,
    ) -> eyre::Result<Option<AsyncTask>> {
        let plugin_name = self.name();
        let mlua::Value::Table(args) = args else {
            return Err(eyre::eyre!("{plugin_name}: unknown argument type"));
        };

        let meta_db = self.meta_db.clone();
        let replicator_tx = get_app_data::<app_data::ReplicatorMarkTx>(lua)
            .wrap_err("App data is not initialized")?
            .0
            .clone();

        let op = MemoryOp::from_str(op)
            .wrap_err_with(|| format!("{plugin_name}: unknown plugin op `{op}`"))?;
        match op {
            MemoryOp::Recall => {
                let key: String = args
                    .get(1)
                    .wrap_err(&format!("{}.recall: missing argument `key`", plugin_name))?;
                let future = Box::pin(async move {
                    let (value, changed) = if key.ends_with("/") {
                        meta_db.get_or_ensure_prefix(&key).await?
                    } else {
                        meta_db.get_or_ensure_key(&key).await?
                    };

                    if changed {
                        let _ = replicator_tx.send(ReplicationMark::Memory {
                            key,
                            value: value.clone(),
                        });
                    }

                    Ok(AsyncTaskResult::JsonValue(value))
                });

                Ok(Some(future))
            }
            MemoryOp::Fetch => {
                let key: String = args
                    .get(1)
                    .wrap_err(&format!("{}.fetch: missing argument `key`", plugin_name))?;
                let future = Box::pin(async move {
                    let (value, changed) = if key.ends_with("/") {
                        meta_db.ensure_prefix(&key).await?
                    } else {
                        meta_db.ensure_key(&key).await?
                    };

                    if changed {
                        let _ = replicator_tx.send(ReplicationMark::Memory {
                            key,
                            value: value.clone(),
                        });
                    }

                    Ok(AsyncTaskResult::JsonValue(value))
                });

                Ok(Some(future))
            }
            MemoryOp::Store => {
                let key: String = args
                    .get(1)
                    .wrap_err(&format!("{}.store: missing argument `key`", plugin_name))?;
                let value: mlua::Value = args
                    .get(2)
                    .wrap_err(&format!("{}.store: missing argument `value`", plugin_name))?;

                if key.ends_with("/") && value.is_nil() {
                    return Err(eyre::eyre!(format!(
                        "{}.store: cannot delete a prefix using `{}.store`. Use `{}.delete`",
                        plugin_name, plugin_name, plugin_name
                    )));
                }

                let value: serde_json::Value = lua.from_value(value).wrap_err(&format!(
                    "{}.store: failed to parse an invalid JSON",
                    plugin_name
                ))?;

                let future = Box::pin(async move {
                    if key.ends_with("/") {
                        meta_db.update_prefix(&key, value.clone()).await?;
                    } else {
                        meta_db.update_key(&key, Some(value.clone())).await?;
                    }

                    let _ = replicator_tx.send(ReplicationMark::Memory { key, value });

                    Ok(AsyncTaskResult::Nil)
                });

                Ok(Some(future))
            }
            MemoryOp::Delete => {
                let key: String = args
                    .get(1)
                    .wrap_err(&format!("{}.delete: missing argument `key`", plugin_name))?;

                let future = Box::pin(async move {
                    if key.ends_with("/") {
                        meta_db.delete_prefix(&key).await?;
                    } else {
                        meta_db.delete_key(&key).await?;
                    }

                    let _ = replicator_tx.send(ReplicationMark::Memory {
                        key,
                        value: serde_json::Value::Null,
                    });

                    Ok(AsyncTaskResult::Nil)
                });

                Ok(Some(future))
            }
        }
    }
}
