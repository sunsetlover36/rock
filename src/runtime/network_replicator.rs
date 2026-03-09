use color_eyre::eyre;
use mlua::LuaSerdeExt;
use shared::{
    EntityData, PlayerKey, WorldSnapshot,
    components::{RadialArea, Vector2D},
};
use slotmap::{SlotMap, new_key_type};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub mod protocol;
use protocol::*;

mod field_registry;
pub(crate) use field_registry::FieldRegistry;

use crate::runtime::{
    GameModeClientApi, LuaResultExt, app_data, get_app_data, get_app_data_mut,
    plugins::entity::components::{
        Blueprint, ComponentKey, Control, Name, OwnedBy, Position, Room, Rotation, Sprite2D,
        SpriteChar,
    },
};

new_key_type! {
    pub(crate) struct PolicyId;
}

type PlayerAnchors = HashMap<PlayerKey, HashSet<hecs::Entity>>;

struct NetworkReplicatorInner {
    entities: HashMap<hecs::Entity, HashSet<EntityDirtyComponent>>,
    memory: HashSet<String>,
    policies: SlotMap<PolicyId, ReplicationPolicy>,
    by_target: HashMap<ReplicationTarget, Vec<PolicyId>>,

    // Pinned policies only
    rooms_policies: HashMap<RoomId, Vec<PolicyId>>,

    player_to_room: HashMap<PlayerKey, RoomId>,
    room_to_players: HashMap<RoomId, Vec<PlayerKey>>,
    player_anchors: PlayerAnchors,
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
                player_to_room: HashMap::new(),
                room_to_players: HashMap::new(),
                player_anchors: HashMap::new(),
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
        let routing = policy.routing;

        let id = inner.policies.insert(policy);
        inner.by_target.entry(target).or_default().push(id);

        if let PolicyRouting::Pinned(room_id) = routing {
            inner.rooms_policies.entry(room_id).or_default().push(id);
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

            if let PolicyRouting::Pinned(room_id) = policy.routing {
                inner
                    .rooms_policies
                    .entry(room_id)
                    .and_modify(|ids| ids.retain(|&id| id != revoked_id));
            }
        }
    }
    pub fn update_policy(
        &self,
        updated_id: PolicyId,
        field: PolicyFieldUpdate,
    ) -> eyre::Result<()> {
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
                PolicyFieldUpdate::Room { id: new_id } => match policy.routing {
                    PolicyRouting::DynamicFollow => {
                        return Err(eyre::eyre!(
                            "Failed to update policy with ID '{:?}': cannot re-route the policy with dynamic follow routing to a new room. to re-route this policy, move the policy target to a new room",
                            updated_id
                        ));
                    }
                    PolicyRouting::Pinned(old_id) => {
                        if old_id == new_id {
                            return Ok(());
                        }

                        rooms_policies
                            .entry(old_id)
                            .and_modify(|ids| ids.retain(|&id| id != updated_id));
                        rooms_policies.entry(new_id).or_default().push(updated_id);

                        policy.routing = PolicyRouting::Pinned(new_id);
                    }
                },
                PolicyFieldUpdate::Throttle { throttle } => {
                    policy.throttle = throttle;
                }
            }
        }

        Ok(())
    }

    pub fn stop_replication(&self, target: &ReplicationTarget) {
        let mut inner = self.inner.borrow_mut();
        if let Some(ids) = inner.by_target.remove(target) {
            for removed_id in ids {
                if let Some(policy) = inner.policies.remove(removed_id) {
                    if let PolicyRouting::Pinned(room_id) = policy.routing {
                        inner
                            .rooms_policies
                            .entry(room_id)
                            .and_modify(|ids| ids.retain(|&id| id != removed_id));
                    }
                }
            }
        }
    }

    pub fn add_player_anchor(&self, pk: PlayerKey, anchor: hecs::Entity) {
        self.inner
            .borrow_mut()
            .player_anchors
            .entry(pk)
            .or_default()
            .insert(anchor);
    }
    pub fn remove_player_anchor(&self, pk: PlayerKey, anchor: hecs::Entity) {
        let mut inner = self.inner.borrow_mut();
        if let Some(anchors) = inner.player_anchors.get_mut(&pk) {
            anchors.retain(|&e| e != anchor);
        }
    }
    pub fn clear_player_anchors(&self, pk: PlayerKey) {
        self.inner.borrow_mut().player_anchors.remove(&pk);
    }
    pub fn set_player_room(&self, pk: PlayerKey, id: Option<RoomId>) {
        let mut inner = self.inner.borrow_mut();

        if let Some(&old_room_id) = inner.player_to_room.get(&pk) {
            inner
                .room_to_players
                .entry(old_room_id)
                .and_modify(|players| players.retain(|&p| p != pk));
        }
        match id {
            Some(id) => {
                inner.player_to_room.insert(pk, id);
                inner.room_to_players.entry(id).or_default().push(pk);
                inner.entities_snapshots.entry(pk).or_default().clear();
            }
            None => {
                inner.player_to_room.remove(&pk);
                inner.entities_snapshots.remove(&pk);
            }
        }
    }

    fn merge_masks_within_area(
        &self,
        room_id: RoomId,
        policy: &ReplicationPolicy,
        world: &hecs::World,
        room_players: &Vec<PlayerKey>,
        area: RadialArea,
        room_masks: &mut HashMap<PlayerKey, u64>,
    ) -> PlayerAnchors {
        let inner = self.inner.borrow();

        let radius_sq = area.radius * area.radius;
        let mut lost_anchors: HashMap<PlayerKey, HashSet<hecs::Entity>> = HashMap::new();

        for &pk in room_players {
            if let Some(anchors) = inner.player_anchors.get(&pk) {
                let mut is_visible = false;
                for &anchor in anchors {
                    if is_visible {
                        break;
                    }

                    let mut query = world.query_one::<(&Room, &Position)>(anchor);
                    if let Ok((room_comp, pos_comp)) = query.get() {
                        if room_comp.0 != room_id {
                            continue;
                        }

                        let anchor_pos = &pos_comp.0;
                        if area.position.distance_squared(anchor_pos) <= radius_sq {
                            is_visible = true;
                        }
                    } else {
                        lost_anchors.entry(pk).or_default().insert(anchor);
                    }
                }

                if is_visible {
                    *room_masks.entry(pk).or_default() |= policy.fields_mask;
                }
            }
        }

        lost_anchors
    }
    fn apply_spatial_filter_for_room(
        &self,
        room_id: RoomId,
        policy: &ReplicationPolicy,
        world: &hecs::World,
        entity_pos: Vector2D,
        fields_masks: &mut HashMap<RoomId, HashMap<PlayerKey, u64>>,
    ) -> Option<PlayerAnchors> {
        let inner = self.inner.borrow();

        if let Some(room_players) = inner.room_to_players.get(&room_id) {
            let room_masks = fields_masks.entry(room_id).or_default();
            match policy.spatial {
                SpatialFilter::Global => {
                    for &pk in room_players {
                        *room_masks.entry(pk).or_default() |= policy.fields_mask;
                    }

                    None
                }
                SpatialFilter::Radius(radius) => Some(self.merge_masks_within_area(
                    room_id,
                    policy,
                    world,
                    room_players,
                    RadialArea {
                        position: entity_pos,
                        radius,
                    },
                    room_masks,
                )),
                SpatialFilter::Area(area) => Some(self.merge_masks_within_area(
                    room_id,
                    policy,
                    world,
                    room_players,
                    area,
                    room_masks,
                )),
            }
        } else {
            None
        }
    }
    pub fn process(&self, lua: &mlua::Lua, tick: u64) -> eyre::Result<()> {
        let lost_anchors = {
            let inner = self.inner.borrow();

            let world =
                get_app_data::<app_data::World>(lua).wrap_err("App data is not initialized")?;
            let mut field_registry =
                get_app_data_mut::<FieldRegistry>(lua).wrap_err("App data is not initialized")?;

            let mut snapshots: HashMap<RoomId, HashMap<PlayerKey, WorldSnapshot>> = HashMap::new();
            let mut lost_anchors: HashMap<PlayerKey, HashSet<hecs::Entity>> = HashMap::new();

            for (&entity, dirty_components) in inner.entities.iter() {
                let mut query = world.query_one::<(
                    &Room,
                    Option<&Blueprint>,
                    &Position,
                    Option<&Rotation>,
                    Option<&Control>,
                    Option<&Name>,
                    Option<&OwnedBy>,
                    Option<&Sprite2D>,
                    Option<&SpriteChar>,
                )>(entity);
                if let Ok(components) = query.get() {
                    let (
                        room_comp,
                        blueprint_comp,
                        pos_comp,
                        rotation_comp,
                        control,
                        name_comp,
                        owned_by_comp,
                        sprite_2d,
                        sprite_char,
                    ) = components;

                    let room_id = room_comp.0;
                    let blueprint_id = blueprint_comp.map(|bp| bp.0);
                    let position = pos_comp.0;
                    let rotation = rotation_comp.map(|c| c.0);
                    let owned_by = owned_by_comp.map(|c| c.0);
                    let custom = get_app_data::<app_data::EntityCustoms>(lua)
                        .wrap_err("App data is not initialized")?
                        .get(&entity)
                        .map(|e| e.clone());

                    let mut fields_masks: HashMap<RoomId, HashMap<PlayerKey, u64>> = HashMap::new();

                    let blueprint_policy_ids = blueprint_id
                        .and_then(|id| inner.by_target.get(&ReplicationTarget::Blueprint(id)))
                        .into_iter()
                        .flatten();
                    let entity_policy_ids = inner
                        .by_target
                        .get(&ReplicationTarget::Entity(entity))
                        .into_iter()
                        .flatten();

                    let policy_ids = blueprint_policy_ids.chain(entity_policy_ids);
                    for &policy_id in policy_ids {
                        if let Some(policy) = inner.policies.get(policy_id) {
                            let recently_lost_anchors: Option<PlayerAnchors>;
                            match policy.routing {
                                PolicyRouting::DynamicFollow => {
                                    recently_lost_anchors = self.apply_spatial_filter_for_room(
                                        room_id,
                                        policy,
                                        &*world,
                                        position,
                                        &mut fields_masks,
                                    );
                                }
                                PolicyRouting::Pinned(pinned_room_id) => {
                                    recently_lost_anchors = self.apply_spatial_filter_for_room(
                                        pinned_room_id,
                                        policy,
                                        &*world,
                                        position,
                                        &mut fields_masks,
                                    );
                                }
                            }

                            if let Some(anchors) = recently_lost_anchors {
                                for (pk, lost) in anchors {
                                    lost_anchors.entry(pk).or_default().extend(lost);
                                }
                            }
                        }
                    }

                    for (&room_id, masks) in fields_masks.iter() {
                        let room_snapshots = snapshots.entry(room_id).or_default();
                        for (&pk, mask) in masks.iter() {
                            let snapshot =
                                room_snapshots.entry(pk).or_insert(WorldSnapshot::new(tick));
                            let mut data = EntityData::default();
                            for comp in dirty_components {
                                match comp {
                                    EntityDirtyComponent::Core(key) => {
                                        let bit = field_registry.get_bit_index(key.as_ref())?;
                                        if (mask & (1 << bit)) == 0 {
                                            continue;
                                        }

                                        match key {
                                            ComponentKey::Name => {
                                                data.name = name_comp.map(|c| c.0.clone());
                                            }
                                            ComponentKey::Position => {
                                                data.position = Some(position);
                                            }
                                            ComponentKey::Rotation => {
                                                data.rotation = rotation;
                                            }
                                            ComponentKey::Control => {
                                                data.speed = control.map(|c| c.speed);
                                            }
                                            ComponentKey::Sprite2D => {
                                                data.sprite = sprite_2d.map(|c| c.0.clone());
                                            }
                                            ComponentKey::SpriteChar => {
                                                data.char = sprite_char.map(|c| c.0.clone());
                                            }
                                            ComponentKey::OwnedBy => {
                                                data.owned_by = owned_by;
                                            }
                                            ComponentKey::Blueprint | ComponentKey::Room => {}
                                        }
                                    }
                                    EntityDirtyComponent::Custom => {
                                        if let Some(custom) = custom.clone() {
                                            data.custom = lua.from_value(mlua::Value::Table(custom)).wrap_err(&format!("Failed to convert a Lua table to JSON object when replicating data for entity with ID {}", entity.id()))?;
                                        }
                                    }
                                }
                            }

                            snapshot.entities.insert(entity.id(), data);
                        }
                    }
                }
            }

            lost_anchors
        };

        // Cleanup
        let mut inner = self.inner.borrow_mut();
        inner.entities.clear();
        inner.memory.clear();
        for (pk, lost) in lost_anchors {
            inner
                .player_anchors
                .entry(pk)
                .and_modify(|anchors| anchors.retain(|anchor| !lost.contains(anchor)));
        }

        Ok(())
    }
}
