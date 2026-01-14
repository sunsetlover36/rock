use dashmap::DashMap;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum MetaValue {
    Pending,
    Stale(Value),
    Fresh(Value),
}

pub struct MetaDb {
    cache: DashMap<String, MetaValue>,
}
impl MetaDb {
    pub fn get(&self, key: &str) -> Option<&MetaValue> {
        self.cache.get(key)
    }
}
