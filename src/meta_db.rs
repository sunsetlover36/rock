use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};

use color_eyre::eyre;
use dashmap::DashMap;
use mlua::{IntoLua, Lua};
use rock_wire::farcaster::Fid;
use serde_json::Value as JsonValue;
use sqlx::{
    Row,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

use crate::{meta_db::json::flatten_json, utils::json_to_lua};

mod json;
use json::insert_nested;

#[derive(Debug, Clone)]
pub enum MetaValue {
    Missing,
    Stale(JsonValue),
    Fresh(JsonValue),
}
impl IntoLua for MetaValue {
    fn into_lua(self, lua: &Lua) -> mlua::Result<mlua::Value> {
        match self {
            MetaValue::Missing => Ok(mlua::Value::Nil),
            MetaValue::Stale(v) => Ok(json_to_lua(lua, v)?),
            MetaValue::Fresh(v) => Ok(json_to_lua(lua, v)?),
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
pub enum MetaDbError {
    Db(sqlx::Error),
    InvalidJson(serde_json::Error),
    InvalidKey { key: String },
    InvalidPrefix { prefix: String },
    Custom(eyre::Report),
}
impl From<MetaDbError> for eyre::ErrReport {
    fn from(err: MetaDbError) -> Self {
        match err {
            MetaDbError::Db(e) => eyre::eyre!("Unknown database error: {}", e),
            MetaDbError::InvalidJson(e) => {
                eyre::eyre!("Database error when trying to parse JSON: {}", e)
            }
            MetaDbError::InvalidKey { key } => {
                eyre::eyre!("Invalid key: expected key, got prefix ({})", key)
            }
            MetaDbError::InvalidPrefix { prefix } => {
                eyre::eyre!("Invalid prefix: expected prefix, got key ({})", prefix)
            }
            MetaDbError::Custom(e) => e,
        }
    }
}

#[derive(Clone)]
pub struct MetaDbConfig {
    pub mode_id: String,
    pub default_ttl: Duration,
}

#[derive(Clone)]
pub struct MetaDb {
    config: MetaDbConfig,
    pool: sqlx::Pool<sqlx::Sqlite>,
    cache: DashMap<String, MetaEntry>,
}
impl MetaDb {
    pub fn farcaster_signer_key(app_fid: Fid, player_fid: Fid) -> String {
        format!("farcaster/signers/{app_fid}/{player_fid}")
    }

    pub async fn new(config: MetaDbConfig) -> Result<Self, sqlx::Error> {
        let db_dir = Path::new("./db");
        fs::create_dir_all(db_dir)?;

        let options = SqliteConnectOptions::new()
            .filename("./db/db.sqlite")
            .create_if_missing(true)
            .foreign_keys(true)
            .pragma("busy_timeout", "5000")
            .pragma("cache_size", "-262144")
            .pragma("synchronous", "NORMAL")
            .pragma("journal_mode", "WAL");

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS meta_kv (
            mode_id TEXT NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL CHECK (json_valid(value)),
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
            PRIMARY KEY (mode_id, key)
            );

            CREATE INDEX IF NOT EXISTS idx_meta_kv_mode_latest ON meta_kv (mode_id, updated_at DESC);
            "#,
        )
        .execute(&pool)
        .await?;

        Ok(Self {
            config,
            pool,
            cache: DashMap::new(),
        })
    }

    fn validate_key(&self, key: &str) -> Result<(), MetaDbError> {
        if key.ends_with("/") {
            Err(MetaDbError::InvalidKey {
                key: key.to_string(),
            })
        } else {
            Ok(())
        }
    }
    fn validate_prefix(&self, prefix: &str) -> Result<(), MetaDbError> {
        if prefix.ends_with("/") {
            Ok(())
        } else {
            Err(MetaDbError::InvalidPrefix {
                prefix: prefix.to_string(),
            })
        }
    }

    pub fn get(&self, key: &str) -> Result<MetaValue, MetaDbError> {
        if key.ends_with("/") {
            let mut map = serde_json::Map::new();
            for e in self.cache.iter() {
                if let Some(key) = e.key().strip_prefix(key)
                    && let Some(value) = &e.value().value
                {
                    insert_nested(&mut map, key, value.clone()).map_err(MetaDbError::Custom)?;
                }
            }

            if map.is_empty() {
                Ok(MetaValue::Missing)
            } else {
                Ok(MetaValue::Fresh(JsonValue::Object(map)))
            }
        } else {
            let entry = match self.cache.get(key) {
                Some(v) => v.clone(),
                None => return Ok(MetaValue::Missing),
            };
            let value = entry.value.unwrap_or(JsonValue::Null);

            let is_stale = entry
                .updated_at
                .checked_add(entry.ttl)
                .map(|expires_at| SystemTime::now() > expires_at)
                .unwrap_or(true);
            if is_stale {
                return Ok(MetaValue::Stale(value));
            }

            Ok(MetaValue::Fresh(value))
        }
    }

    fn update_cache(&self, key: &str, new_value: Option<JsonValue>) -> bool {
        let now = SystemTime::now();

        match self.cache.entry(key.to_owned()) {
            dashmap::Entry::Occupied(mut e) => {
                let entry = e.get_mut();
                let changed = new_value != entry.value;

                entry.updated_at = now;
                entry.value = new_value;

                changed
            }
            dashmap::Entry::Vacant(e) => {
                let changed = new_value.is_some();

                e.insert(MetaEntry {
                    ttl: self.config.default_ttl,
                    updated_at: now,
                    value: new_value,
                });

                changed
            }
        }
    }

    pub async fn ensure_key(&self, key: &str) -> Result<(JsonValue, bool), MetaDbError> {
        self.validate_key(key)?;

        let raw_str: Option<String> =
            sqlx::query_scalar("SELECT value FROM meta_kv WHERE mode_id = ? AND key = ?")
                .bind(self.config.mode_id.clone())
                .bind(key)
                .fetch_optional(&self.pool)
                .await
                .map_err(MetaDbError::Db)?;

        match raw_str {
            Some(raw_str) => {
                let json_value: JsonValue =
                    serde_json::from_str(&raw_str).map_err(MetaDbError::InvalidJson)?;

                let changed = self.update_cache(key, Some(json_value.clone()));
                Ok((json_value.clone(), changed))
            }
            None => {
                let changed = self.update_cache(key, None);
                Ok((serde_json::Value::Null, changed))
            }
        }
    }

    pub async fn ensure_prefix(&self, prefix: &str) -> Result<(JsonValue, bool), MetaDbError> {
        self.validate_prefix(prefix)?;

        let rows =
            sqlx::query("SELECT key, value FROM meta_kv WHERE mode_id = ? AND key LIKE ? || '%'")
                .bind(self.config.mode_id.clone())
                .bind(prefix)
                .fetch_all(&self.pool)
                .await
                .map_err(MetaDbError::Db)?;

        let mut map = serde_json::Map::new();
        for row in rows {
            let key: String = row.get("key");
            let value_str: String = row.get("value");

            if let Ok(json_value) = serde_json::from_str(&value_str)
                && let Some(stripped_prefix) = key.strip_prefix(prefix)
            {
                insert_nested(
                    &mut map,
                    stripped_prefix.trim_start_matches('/'),
                    json_value,
                )
                .map_err(MetaDbError::Custom)?;
            }
        }

        if map.is_empty() {
            let changed = self.update_cache(prefix, None);
            Ok((serde_json::Value::Null, changed))
        } else {
            let json_map = JsonValue::Object(map);
            let changed = self.update_cache(prefix, Some(json_map.clone()));
            Ok((json_map, changed))
        }
    }

    pub async fn get_or_ensure_key(&self, key: &str) -> Result<(JsonValue, bool), MetaDbError> {
        self.validate_key(key)?;

        match self.get(key)? {
            MetaValue::Missing | MetaValue::Stale(_) => self.ensure_key(key).await,
            MetaValue::Fresh(v) => Ok((v, false)),
        }
    }
    pub async fn get_or_ensure_prefix(
        &self,
        prefix: &str,
    ) -> Result<(JsonValue, bool), MetaDbError> {
        self.validate_prefix(prefix)?;

        match self.get(prefix)? {
            MetaValue::Missing | MetaValue::Stale(_) => self.ensure_prefix(prefix).await,
            MetaValue::Fresh(v) => Ok((v, false)),
        }
    }

    pub async fn update_key(&self, key: &str, value: Option<JsonValue>) -> Result<(), MetaDbError> {
        self.validate_key(key)?;

        let value_str = match &value {
            Some(v) => serde_json::to_string(&v).map_err(MetaDbError::InvalidJson)?,
            None => "null".to_string(),
        };

        sqlx::query(
            r#"
            INSERT INTO meta_kv (mode_id, key, value)
            VALUES (?, ?, ?)
            ON CONFLICT (mode_id, key)
            DO UPDATE SET
                value = excluded.value,
                updated_at = unixepoch()
            "#,
        )
        .bind(self.config.mode_id.clone())
        .bind(key)
        .bind(value_str)
        .execute(&self.pool)
        .await
        .map_err(MetaDbError::Db)?;

        self.update_cache(key, value);

        Ok(())
    }
    pub async fn update_prefix(&self, prefix: &str, value: JsonValue) -> Result<(), MetaDbError> {
        self.validate_prefix(prefix)?;

        let mut kvs: Vec<(String, JsonValue)> = Vec::new();
        flatten_json(prefix, value.clone(), &mut kvs);

        let mut tx = self.pool.begin().await.map_err(MetaDbError::Db)?;

        sqlx::query("DELETE FROM meta_kv WHERE mode_id = ? AND key LIKE ?")
            .bind(&self.config.mode_id)
            .bind(format!("{}%", prefix))
            .execute(&mut *tx)
            .await
            .map_err(MetaDbError::Db)?;
        for (k, v) in &kvs {
            let value_str = serde_json::to_string(&v).map_err(MetaDbError::InvalidJson)?;
            sqlx::query(
                r#"
                    INSERT INTO meta_kv (mode_id, key, value)
                    VALUES (?, ?, ?)
                    ON CONFLICT (mode_id, key)
                    DO UPDATE SET
                        value = excluded.value,
                        updated_at = unixepoch() 
                "#,
            )
            .bind(&self.config.mode_id)
            .bind(k.clone())
            .bind(value_str)
            .execute(&mut *tx)
            .await
            .map_err(MetaDbError::Db)?;
        }
        tx.commit().await.map_err(MetaDbError::Db)?;

        self.cache.retain(|key, _| !key.starts_with(prefix));
        for (k, v) in kvs {
            self.update_cache(&k, Some(v));
        }

        Ok(())
    }

    pub async fn delete_key(&self, key: &str) -> Result<(), MetaDbError> {
        sqlx::query("DELETE FROM meta_kv WHERE mode_id = ? AND key = ?")
            .bind(self.config.mode_id.clone())
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(MetaDbError::Db)?;
        self.cache.remove(key);

        Ok(())
    }
    pub async fn delete_prefix(&self, prefix: &str) -> Result<(), MetaDbError> {
        self.validate_prefix(prefix)?;

        sqlx::query("DELETE FROM meta_kv WHERE mode_id = ? AND key LIKE ?")
            .bind(self.config.mode_id.clone())
            .bind(format!("{}%", prefix))
            .execute(&self.pool)
            .await
            .map_err(MetaDbError::Db)?;
        self.cache.retain(|key, _| !key.starts_with(prefix));

        Ok(())
    }
}
