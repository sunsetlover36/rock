use std::{str::FromStr, sync::Arc};

use color_eyre::eyre::{self, Context};
use mlua::{Function, Lua, LuaSerdeExt, Table};
use strum::{AsRefStr, Display, EnumString};

use super::{
    Yielder,
    protocol::{AsyncTask, AsyncTaskResult, GameModePlugin, PluginName},
};
use crate::{
    meta_db::MetaDb,
    runtime::{
        app_data, get_app_data,
        network_replicator::protocol::{ReplicationMark, ReplicationTarget},
        utils::LuaResultExt,
    },
    rx::RxSync,
};

#[derive(Debug, Clone, Copy, EnumString, Display, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum MemoryOp {
    Fetch,
    Recall,
    Store,
    Delete,
}

pub struct MemoryPlugin {
    pub meta_db: Arc<MetaDb>,
}
impl GameModePlugin for MemoryPlugin {
    fn name(&self) -> PluginName {
        PluginName::Memory
    }

    fn create_global_api(&self, lua: &Lua) -> mlua::Result<Option<Table>> {
        let plugin_name = self.name();
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

        let node_fn = lua.create_function(|_, key: String| {
            Ok(RxSync::new(ReplicationTarget::MemoryNode(key)))
        })?;
        table.set("node", node_fn)?;

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

        let store_op = format!("{}_{}", &name_in_uppercase, MemoryOp::Store);
        let store_fn = yielder_fn.call::<Function>(store_op)?;
        table.set("store", store_fn)?;

        Ok(Some(table))
    }
    fn handle_op(
        &self,
        lua: &mlua::Lua,
        op: &str,
        args: mlua::Table,
    ) -> eyre::Result<Option<AsyncTask>> {
        let plugin_name = self.name();
        let meta_db = self.meta_db.clone();

        let replicator_tx = get_app_data::<app_data::ReplicatorMarkTx>(lua)
            .wrap_err("App data is not initialized")?
            .0
            .clone();

        let op =
            MemoryOp::from_str(op).wrap_err_with(|| format!("Unknown memory plugin op: {}", op))?;
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
                        let _ = replicator_tx.send(ReplicationMark::Memory { key, value });
                    } else {
                        meta_db.update_key(&key, Some(value.clone())).await?;
                        let _ = replicator_tx.send(ReplicationMark::Memory { key, value });
                    }

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
