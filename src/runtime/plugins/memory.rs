use std::{str::FromStr, sync::Arc};

use color_eyre::eyre::{self, Context};
use mlua::{Lua, LuaSerdeExt, Table};
use strum::{AsRefStr, Display, EnumString};

use super::protocol::{AsyncTask, AsyncTaskResult, GameModePlugin, PluginName};
use crate::{
    meta_db::MetaDb,
    runtime::{
        app_data, get_app_data,
        network_replicator::protocol::ReplicationMark,
        plugins::{build_plugin_op, yield_op},
        utils::LuaResultExt,
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
// Read related materialized nodes after a mutation, including prefix views.
async fn read_node(
    meta_db: &MetaDb,
    key: &str,
    cached: bool,
) -> eyre::Result<Option<serde_json::Value>> {
    if key.ends_with('/') {
        let (value, _) = if cached {
            meta_db.get_or_ensure_prefix(key).await?
        } else {
            meta_db.ensure_prefix(key).await?
        };
        Ok((!value.is_null()).then_some(value))
    } else {
        let (value, _) = if cached {
            meta_db.get_or_ensure_key_value(key).await?
        } else {
            meta_db.ensure_key(key).await?
        };
        Ok(value)
    }
}
async fn publish_nodes(
    meta_db: &MetaDb,
    tx: &flume::Sender<ReplicationMark>,
    keys: Vec<String>,
) -> eyre::Result<()> {
    for key in keys {
        let value = read_node(meta_db, &key, false).await?;
        publish_value(tx, key, value);
    }
    Ok(())
}
fn publish_value(
    tx: &flume::Sender<ReplicationMark>,
    key: String,
    value: Option<serde_json::Value>,
) {
    let mark = match value {
        Some(value) => ReplicationMark::Memory { key, value },
        None => ReplicationMark::MemoryDelete { key },
    };
    let _ = tx.send(mark);
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
                let op = build_plugin_op(&lua, opcode, mlua::Value::Table(args))?;
                yield_op(&lua, "memory.recall", op).await
            }
        })?;
        table.set("recall", recall_fn)?;

        let fetch_op = format!("{}_{}", &name_in_uppercase, MemoryOp::Fetch);
        let fetch_fn = lua.create_async_function(move |lua, key: String| {
            let opcode = fetch_op.clone();

            async move {
                let args = lua.create_sequence_from([key])?;
                let op = build_plugin_op(&lua, opcode, mlua::Value::Table(args))?;
                yield_op(&lua, "memory.fetch", op).await
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

                    let op = build_plugin_op(&lua, opcode, mlua::Value::Table(args))?;
                    yield_op(&lua, "memory.store", op).await
                }
            })?;
        table.set("store", store_fn)?;

        let delete_op = format!("{}_{}", &name_in_uppercase, MemoryOp::Delete);
        let delete_fn = lua.create_async_function(move |lua, key: String| {
            let opcode = delete_op.clone();

            async move {
                let args = lua.create_sequence_from([key])?;
                let op = build_plugin_op(&lua, opcode, mlua::Value::Table(args))?;
                yield_op(&lua, "memory.delete", op).await
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
            MemoryOp::Recall | MemoryOp::Fetch => {
                let method = if matches!(op, MemoryOp::Recall) {
                    "recall"
                } else {
                    "fetch"
                };
                let key: String = args
                    .get(1)
                    .wrap_err(&format!("{plugin_name}.{method}: missing argument `key`"))?;
                let future = Box::pin(async move {
                    let value = read_node(&meta_db, &key, matches!(op, MemoryOp::Recall)).await?;
                    // Hydrate replication even when the DB cache is already fresh.
                    publish_value(&replicator_tx, key, value.clone());
                    Ok(AsyncTaskResult::JsonValue(
                        value.unwrap_or(serde_json::Value::Null),
                    ))
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

                let nodes = get_app_data::<app_data::NetworkReplicator>(lua)
                    .wrap_err("Network replicator is not initialized")?
                    .0
                    .affected_memory_nodes(&key);
                let future = Box::pin(async move {
                    if key.ends_with("/") {
                        meta_db.update_prefix(&key, value.clone()).await?;
                    } else {
                        meta_db.update_key(&key, Some(value.clone())).await?;
                    }

                    publish_nodes(&meta_db, &replicator_tx, nodes).await?;

                    Ok(AsyncTaskResult::Nil)
                });

                Ok(Some(future))
            }
            MemoryOp::Delete => {
                let key: String = args
                    .get(1)
                    .wrap_err(&format!("{}.delete: missing argument `key`", plugin_name))?;

                let nodes = get_app_data::<app_data::NetworkReplicator>(lua)
                    .wrap_err("Network replicator is not initialized")?
                    .0
                    .affected_memory_nodes(&key);
                let future = Box::pin(async move {
                    if key.ends_with("/") {
                        meta_db.delete_prefix(&key).await?;
                    } else {
                        meta_db.delete_key(&key).await?;
                    }

                    // A prefix deletion also invalidates cached descendant snapshots.
                    let _ = replicator_tx.send(ReplicationMark::MemoryDelete { key });
                    publish_nodes(&meta_db, &replicator_tx, nodes).await?;

                    Ok(AsyncTaskResult::Nil)
                });

                Ok(Some(future))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn stored_null_remains_present_and_missing_key_produces_removal() {
        let db = MetaDb::test_db().await;
        let (tx, rx) = flume::unbounded();
        db.update_key("nullable", Some(serde_json::Value::Null))
            .await
            .unwrap();
        for cached in [false, true] {
            let value = read_node(&db, "nullable", cached).await.unwrap();
            assert_eq!(value, Some(serde_json::Value::Null));
            publish_value(&tx, "nullable".into(), value);
            assert!(matches!(
                rx.recv().unwrap(),
                ReplicationMark::Memory {
                    value: serde_json::Value::Null,
                    ..
                }
            ));
        }
        db.delete_key("nullable").await.unwrap();
        publish_nodes(&db, &tx, vec!["nullable".into()])
            .await
            .unwrap();
        assert!(
            matches!(rx.recv().unwrap(), ReplicationMark::MemoryDelete { key } if key == "nullable")
        );
    }

    #[tokio::test]
    async fn refresh_updates_prefix_views_and_withdraws_leaves_removed_by_prefix_replace() {
        let db = MetaDb::test_db().await;
        let (tx, rx) = flume::unbounded();
        db.update_key("profile/name", Some(json!("Alice")))
            .await
            .unwrap();
        db.update_key("profile/hp", Some(json!(100))).await.unwrap();
        let nodes = vec![
            "profile/".into(),
            "profile/name".into(),
            "profile/hp".into(),
        ];
        publish_nodes(&db, &tx, nodes.clone()).await.unwrap();
        assert!(
            matches!(rx.recv().unwrap(), ReplicationMark::Memory { value, .. } if value == json!({ "name": "Alice", "hp": 100 }))
        );
        rx.drain().for_each(drop);
        db.update_prefix("profile/", json!({ "name": "Bob" }))
            .await
            .unwrap();
        publish_nodes(&db, &tx, nodes.clone()).await.unwrap();
        assert!(
            matches!(rx.recv().unwrap(), ReplicationMark::Memory { value, .. } if value == json!({ "name": "Bob" }))
        );
        assert!(
            matches!(rx.recv().unwrap(), ReplicationMark::Memory { value, .. } if value == json!("Bob"))
        );
        assert!(
            matches!(rx.recv().unwrap(), ReplicationMark::MemoryDelete { key } if key == "profile/hp")
        );
        db.delete_prefix("profile/").await.unwrap();
        publish_nodes(&db, &tx, nodes).await.unwrap();
        assert!(
            rx.drain()
                .all(|mark| matches!(mark, ReplicationMark::MemoryDelete { .. }))
        );
    }
}
