use std::collections::{HashMap, HashSet};

pub mod protocol;
use protocol::*;

pub(super) struct NetworkReplicator {
    entities: HashMap<hecs::Entity, HashSet<EntityDirtyComponent>>,
    memory: HashSet<String>,
    policies: HashMap<ReplicationTarget, Vec<ReplicationPolicy>>,
}
impl NetworkReplicator {
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
            memory: HashSet::new(),
            policies: HashMap::new(),
        }
    }

    pub fn mark(&mut self, mark: ReplicationMark) {
        match mark {
            ReplicationMark::Entity { id, component } => {
                self.entities.entry(id).or_default().insert(component);
            }
            ReplicationMark::Memory(key) => {
                self.memory.insert(key);
            }
        }
    }

    pub fn batch(&mut self) {}
}
