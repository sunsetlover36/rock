use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub mod protocol;
use protocol::*;
use slotmap::{SlotMap, new_key_type};

use crate::runtime::GameModeClientApi;

new_key_type! {
    pub(crate) struct PolicyId;
}

struct NetworkReplicatorInner {
    entities: HashMap<hecs::Entity, HashSet<EntityDirtyComponent>>,
    memory: HashSet<String>,
    policies: SlotMap<PolicyId, ReplicationPolicy>,
    by_target: HashMap<ReplicationTarget, Vec<PolicyId>>,
    signals: Vec<PendingSignal>,
    client_api: Arc<dyn GameModeClientApi>,
}

pub(crate) struct NetworkReplicator {
    inner: RefCell<NetworkReplicatorInner>,
}
impl NetworkReplicator {
    pub fn new(client_api: Arc<dyn GameModeClientApi>) -> Self {
        Self {
            inner: RefCell::new(NetworkReplicatorInner {
                entities: HashMap::new(),
                memory: HashSet::new(),
                policies: SlotMap::<PolicyId, ReplicationPolicy>::with_key(),
                by_target: HashMap::new(),
                signals: Vec::new(),
                client_api,
            }),
        }
    }

    pub fn schedule_signal(&self, signal: PendingSignal) {
        self.inner.borrow_mut().signals.push(signal);
    }

    pub fn mark(&self, mark: ReplicationMark) {
        let mut inner = self.inner.borrow_mut();
        match mark {
            ReplicationMark::Entity { id, component } => {
                inner.entities.entry(id).or_default().insert(component);
            }
            ReplicationMark::Memory(key) => {
                inner.memory.insert(key);
            }
        }
    }

    pub fn commit_policy(&self, policy: ReplicationPolicy) -> PolicyId {
        let target = policy.target.clone();
        let mut inner = self.inner.borrow_mut();
        let id = inner.policies.insert(policy);
        inner.by_target.entry(target).or_default().push(id);

        id
    }
    pub fn revoke_policy(&self, id: PolicyId) {
        let mut inner = self.inner.borrow_mut();
        if let Some(policy) = inner.policies.remove(id) {
            inner
                .by_target
                .entry(policy.target)
                .and_modify(|policies| policies.retain(|&policy_id| policy_id != id));
        }
    }

    pub fn update_policy(&self, id: PolicyId, field: PolicyFieldUpdate) {
        let mut inner = self.inner.borrow_mut();
        if let Some(policy) = inner.policies.get_mut(id) {
            match field {
                PolicyFieldUpdate::Spatial { filter } => {
                    policy.spatial = filter;
                }
                PolicyFieldUpdate::Room { name } => {
                    policy.room = name;
                }
                PolicyFieldUpdate::Throttle { throttle } => {
                    policy.throttle = throttle;
                }
            }
        }
    }

    pub fn stop_replication(&self, target: &ReplicationTarget) {
        let mut inner = self.inner.borrow_mut();
        if let Some(keys) = inner.by_target.remove(target) {
            for key in keys {
                inner.policies.remove(key);
            }
        }
    }

    pub fn process(&self) {}
}
