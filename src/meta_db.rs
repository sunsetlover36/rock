use std::time::{Duration, SystemTime};

use dashmap::DashMap;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;

#[derive(Debug, Clone)]
pub enum MetaValue {
    Missing,
    Pending,
    Stale(Option<Value>),
    Fresh(Option<Value>),
}

#[derive(Debug, Clone)]
pub struct MetaEntry {
    ttl: u64,
    updated_at: SystemTime,
    is_scheduled: bool,
    value: Option<Value>,
}

#[derive(Debug)]
pub enum MetaEnsureError {
    Db(sqlx::Error),
    InvalidJson(serde_json::Error),
}

pub struct MetaDbConfig {
    mode_id: String,
    default_ttl: u64,
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
            .connect("sqlite://db.sqlite")
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
        if entry.is_scheduled {
            return MetaValue::Pending;
        }

        let is_stale = entry
            .updated_at
            .checked_add(Duration::from_secs(entry.ttl))
            .map(|expires_at| SystemTime::now() > expires_at)
            .unwrap_or(true);
        if is_stale {
            return MetaValue::Stale(entry.value);
        }

        MetaValue::Fresh(entry.value)
    }

    fn update_entry(&self, key: &str, value: Option<Value>) {
        let now = SystemTime::now();

        match self.cache.entry(key.to_owned()) {
            dashmap::Entry::Occupied(mut e) => {
                let entry = e.get_mut();
                entry.updated_at = now;
                entry.is_scheduled = false;
                entry.value = value;
            }
            dashmap::Entry::Vacant(e) => {
                e.insert(MetaEntry {
                    ttl: self.config.default_ttl,
                    updated_at: now,
                    is_scheduled: false,
                    value,
                });
            }
        }
    }
    pub async fn ensure_key(&self, key: &str) -> Result<Option<Value>, MetaEnsureError> {
        let raw_str: Option<String> =
            sqlx::query_scalar("SELECT value FROM meta_kv WHERE mode_id = ? AND key = ?")
                .bind(self.config.mode_id.clone())
                .bind(key)
                .fetch_optional(&self.pool)
                .await
                .map_err(MetaEnsureError::Db)?;

        match raw_str {
            Some(raw_str) => {
                let json: Value =
                    serde_json::from_str(&raw_str).map_err(MetaEnsureError::InvalidJson)?;
                let json = Some(json);

                self.update_entry(key, json.clone());
                Ok(json.clone())
            }
            None => {
                self.update_entry(key, None);
                Ok(None)
            }
        }
    }

    pub async fn ensure_prefix(
        &self,
        key: &str,
    ) -> Result<Option<Vec<(String, Value)>>, MetaEnsureError> {
        // Get COUNT of keys under the prefix and iterate? Get all keys under the prefix?
        // Fetch the first level vs fetch all children prefixes too (shallow prefix)
        // TODO: Customizable delimiters (meta:player:1:items vs. meta/player/1/items)
        // Default delimiter right now: `/`
        unimplemented!();
    }
}
