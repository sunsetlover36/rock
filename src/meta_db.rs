use std::time::{Duration, SystemTime};

use color_eyre::eyre;
use dashmap::DashMap;
use mlua::{IntoLua, Lua};
use serde_json::Value as JsonValue;
use sqlx::{Row, sqlite::SqlitePoolOptions};

use crate::utils::json_to_lua;

#[derive(Debug, Clone)]
pub enum MetaValue {
    Missing,
    Stale(Option<JsonValue>),
    Fresh(Option<JsonValue>),
}
impl IntoLua for MetaValue {
    fn into_lua(self, lua: &Lua) -> mlua::Result<mlua::Value> {
        match self {
            MetaValue::Missing => Ok(mlua::Value::Nil),
            MetaValue::Stale(v) => Ok(match v {
                Some(v) => json_to_lua(lua, v)?,
                None => mlua::Value::Nil,
            }),
            MetaValue::Fresh(v) => Ok(match v {
                Some(v) => json_to_lua(lua, v)?,
                None => mlua::Value::Nil,
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MetaEntry {
    ttl: Duration,
    updated_at: SystemTime,
    value: Option<JsonValue>,
}

#[derive(Debug)]
pub enum MetaEnsureError {
    Db(sqlx::Error),
    InvalidJson(serde_json::Error),
}
impl From<MetaEnsureError> for eyre::ErrReport {
    fn from(err: MetaEnsureError) -> Self {
        match err {
            MetaEnsureError::Db(e) => eyre::eyre!("Unknown database error: {}", e),
            MetaEnsureError::InvalidJson(e) => {
                eyre::eyre!("Database error when trying to parse JSON: {}", e)
            }
        }
    }
}

pub struct MetaDbConfig {
    pub mode_id: String,
    pub default_ttl: Duration,
}

pub struct MetaDb {
    config: MetaDbConfig,
    pool: sqlx::Pool<sqlx::Sqlite>,
    cache: DashMap<String, MetaEntry>,
}
impl MetaDb {
    pub async fn new(config: MetaDbConfig) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .after_connect(|conn, _| {
                Box::pin(async move {
                    for pragma in [
                        "PRAGMA foreign_keys = ON;",
                        "PRAGMA busy_timeout = 5000;",
                        "PRAGMA cache_size = -262144;",
                        "PRAGMA synchronous = NORMAL;",
                        "PRAGMA journal_mode = WAL;",
                    ] {
                        sqlx::query(pragma).execute(&mut *conn).await?;
                    }

                    Ok(())
                })
            })
            .connect("sqlite://db.sqlite?mode=rwc")
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self {
            config,
            pool,
            cache: DashMap::new(),
        })
    }

    pub fn get(&self, key: &str) -> MetaValue {
        let entry = match self.cache.get(key) {
            Some(v) => v.clone(),
            None => return MetaValue::Missing,
        };

        let is_stale = entry
            .updated_at
            .checked_add(entry.ttl)
            .map(|expires_at| SystemTime::now() > expires_at)
            .unwrap_or(true);
        if is_stale {
            return MetaValue::Stale(entry.value);
        }

        MetaValue::Fresh(entry.value)
    }

    fn update_entry(&self, key: &str, value: Option<JsonValue>) {
        let now = SystemTime::now();

        match self.cache.entry(key.to_owned()) {
            dashmap::Entry::Occupied(mut e) => {
                let entry = e.get_mut();
                entry.updated_at = now;
                entry.value = value;
            }
            dashmap::Entry::Vacant(e) => {
                e.insert(MetaEntry {
                    ttl: self.config.default_ttl,
                    updated_at: now,
                    value,
                });
            }
        }
    }

    pub async fn ensure_key(&self, key: &str) -> Result<Option<JsonValue>, MetaEnsureError> {
        let raw_str: Option<String> =
            sqlx::query_scalar("SELECT value FROM meta_kv WHERE mode_id = ? AND key = ?")
                .bind(self.config.mode_id.clone())
                .bind(key)
                .fetch_optional(&self.pool)
                .await
                .map_err(MetaEnsureError::Db)?;

        match raw_str {
            Some(raw_str) => {
                let json_value: Option<JsonValue> =
                    Some(serde_json::from_str(&raw_str).map_err(MetaEnsureError::InvalidJson)?);

                self.update_entry(key, json_value.clone());
                Ok(json_value.clone())
            }
            None => {
                self.update_entry(key, None);
                Ok(None)
            }
        }
    }

    pub async fn ensure_prefix(&self, prefix: &str) -> Result<Option<JsonValue>, MetaEnsureError> {
        let rows =
            sqlx::query("SELECT key, value FROM meta_kv WHERE mode_id = ? AND key LIKE ? || '%'")
                .bind(self.config.mode_id.clone())
                .bind(prefix)
                .fetch_all(&self.pool)
                .await
                .map_err(MetaEnsureError::Db)?;

        let mut map = serde_json::Map::new();
        for row in rows {
            let key: String = row.get("key");
            let value_str: String = row.get("value");

            if let Ok(json_value) = serde_json::from_str(&value_str) {
                if let Some(stripped_prefix) = key.strip_prefix(prefix) {
                    map.insert(stripped_prefix.to_string(), json_value);
                }
            }
        }

        if map.is_empty() {
            self.update_entry(prefix, None);
            Ok(None)
        } else {
            let json_map = JsonValue::Object(map);
            self.update_entry(prefix, Some(json_map.clone()));
            Ok(Some(json_map))
        }
    }

    pub async fn get_or_ensure_key(&self, key: &str) -> Result<Option<JsonValue>, MetaEnsureError> {
        match self.get(key) {
            MetaValue::Missing | MetaValue::Stale(_) => self.ensure_key(key).await,
            MetaValue::Fresh(v) => Ok(v),
        }
    }
    pub async fn get_or_ensure_prefix(
        &self,
        prefix: &str,
    ) -> Result<Option<JsonValue>, MetaEnsureError> {
        match self.get(prefix) {
            MetaValue::Missing | MetaValue::Stale(_) => self.ensure_prefix(prefix).await,
            MetaValue::Fresh(v) => Ok(v),
        }
    }
}
