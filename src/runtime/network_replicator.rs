use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub mod protocol;
use color_eyre::eyre;
use protocol::*;
use shared::PlayerKey;
use slotmap::{SlotMap, new_key_type};

use crate::runtime::{
    GameModeClientApi, LuaResultExt, app_data, get_app_data, plugins::entity::components::Blueprint,
};

new_key_type! {
    pub(crate) struct PolicyId;
}

struct NetworkReplicatorInner {
    entities: HashMap<hecs::Entity, HashSet<EntityDirtyComponent>>,
    memory: HashSet<String>,
    policies: SlotMap<PolicyId, ReplicationPolicy>,
    by_target: HashMap<ReplicationTarget, Vec<PolicyId>>,
    rooms_policies: HashMap<RoomId, Vec<PolicyId>>,
    player_rooms: HashMap<PlayerKey, RoomId>,
    entities_snapshots: HashMap<PlayerKey, HashSet<hecs::Entity>>,
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
                rooms_policies: HashMap::new(),
                player_rooms: HashMap::new(),
                entities_snapshots: HashMap::new(),
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
        let mut inner = self.inner.borrow_mut();
        let target = policy.target.clone();
        let room = policy.room;

        let id = inner.policies.insert(policy);
        inner.by_target.entry(target).or_default().push(id);

        if let Some(room) = room {
            inner.rooms_policies.entry(room).or_default().push(id);
        }

        id
    }
    pub fn revoke_policy(&self, revoked_id: PolicyId) {
        let mut inner = self.inner.borrow_mut();
        if let Some(policy) = inner.policies.remove(revoked_id) {
            inner
                .by_target
                .entry(policy.target)
                .and_modify(|policies| policies.retain(|&id| id != revoked_id));

            if let Some(room) = policy.room {
                inner
                    .rooms_policies
                    .entry(room)
                    .and_modify(|ids| ids.retain(|&id| id != revoked_id));
            }
        }
    }

    pub fn update_policy(&self, updated_id: PolicyId, field: PolicyFieldUpdate) {
        let NetworkReplicatorInner {
            policies,
            rooms_policies,
            ..
        } = &mut *self.inner.borrow_mut();

        if let Some(policy) = policies.get_mut(updated_id) {
            match field {
                PolicyFieldUpdate::Spatial { filter } => {
                    policy.spatial = filter;
                }
                PolicyFieldUpdate::Room { id: new_id } => {
                    let old_id = policy.room;
                    if old_id == new_id {
                        return;
                    }

                    if let Some(old_id) = old_id {
                        rooms_policies
                            .entry(old_id)
                            .and_modify(|ids| ids.retain(|&id| id != updated_id));
                    }

                    if let Some(new_id) = new_id {
                        rooms_policies.entry(new_id).or_default().push(updated_id);
                    }

                    policy.room = new_id;
                }
                PolicyFieldUpdate::Throttle { throttle } => {
                    policy.throttle = throttle;
                }
            }
        }
    }

    pub fn stop_replication(&self, target: &ReplicationTarget) {
        let mut inner = self.inner.borrow_mut();
        if let Some(ids) = inner.by_target.remove(target) {
            for removed_id in ids {
                if let Some(policy) = inner.policies.remove(removed_id) {
                    if let Some(room) = policy.room {
                        inner
                            .rooms_policies
                            .entry(room)
                            .and_modify(|ids| ids.retain(|&id| id != removed_id));
                    }
                }
            }
        }
    }

    pub fn set_player_room(&self, pk: PlayerKey, id: Option<RoomId>) {
        let mut inner = self.inner.borrow_mut();

        match id {
            Some(id) => {
                inner.player_rooms.insert(pk, id);
                inner.entities_snapshots.entry(pk).or_default().clear();
            }
            None => {
                inner.player_rooms.remove(&pk);
                inner.entities_snapshots.remove(&pk);
            }
        }
    }

    pub fn process(&self, lua: &mlua::Lua) -> eyre::Result<()> {
        let inner = self.inner.borrow();
        let world = get_app_data::<app_data::World>(lua).wrap_err("App data is not initialized")?;

        for (entity, components) in inner.entities.iter() {
            let entity = entity.clone();
            let blueprint_id = world.get::<&Blueprint>(entity).ok().map(|b| b.0);

            let blueprint_policies =
                blueprint_id.and_then(|id| inner.by_target.get(&ReplicationTarget::Blueprint(id)));
            let entity_policies = inner
                .by_target
                .get(&ReplicationTarget::Entity(entity.clone()));

            let policies = blueprint_policies.into_iter().concat();
        }

        {
            let mut inner = self.inner.borrow_mut();
            inner.entities.clear();
            inner.memory.clear();
        }

        Ok(())
    }
}
