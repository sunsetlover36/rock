use color_eyre::eyre;
use mlua::LuaSerdeExt;
use shared::{
    EntityData, PlayerKey, WorldSnapshot,
    components::{RadialArea, Vector2D},
};
use slotmap::SlotMap;
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
    plugins::entity::components::{Blueprint, ComponentData, ComponentKey, Position, Room},
};

type PlayerAnchors = HashMap<PlayerKey, HashSet<hecs::Entity>>;

struct NetworkReplicatorInner {
    entities: HashMap<hecs::Entity, Vec<EntityDirtyComponent>>,
    memory: HashMap<String, serde_json::Value>,
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
    mark_tx: flume::Sender<ReplicationMark>,
    mark_rx: flume::Receiver<ReplicationMark>,
}
impl NetworkReplicator {
    pub fn new(client_api: Arc<dyn GameModeClientApi>) -> Self {
        let (mark_tx, mark_rx) = flume::unbounded::<ReplicationMark>();

        Self {
            inner: RefCell::new(NetworkReplicatorInner {
                entities: HashMap::new(),
                memory: HashMap::new(),
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
            mark_tx,
            mark_rx,
        }
    }

    pub fn get_mark_tx(&self) -> flume::Sender<ReplicationMark> {
        self.mark_tx.clone()
    }

    pub fn schedule_signal(&self, signal: PendingSignal) {
        self.inner.borrow_mut().signals.push(signal);
    }

    fn mark_update(&self, mark: ReplicationMark) {
        let mut inner = self.inner.borrow_mut();
        match mark {
            ReplicationMark::Entity { id, component } => {
                inner.entities.entry(id).or_default().push(component);
            }
            ReplicationMark::Memory { key, value } => {
                inner.memory.insert(key, value);
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

    // Returns lost (non-existing) player anchors to clean them up
    fn merge_mask_within_area(
        &self,
        room_id: RoomId,
        players: &Vec<PlayerKey>,
        mask: u64,
        world: &hecs::World,
        area: RadialArea,
        room_masks: &mut HashMap<PlayerKey, u64>,
    ) -> PlayerAnchors {
        let inner = self.inner.borrow();

        let radius_sq = area.radius * area.radius;
        let mut lost_anchors: HashMap<PlayerKey, HashSet<hecs::Entity>> = HashMap::new();

        for &pk in players {
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
                    *room_masks.entry(pk).or_default() |= mask;
                }
            }
        }

        lost_anchors
    }

    // Applies a spatial filter for the selected room using a policy fields mask
    fn apply_spatial_filter_for_room(
        &self,
        room_id: RoomId,
        players: &Vec<PlayerKey>,
        policy: &ReplicationPolicy,
        world: &hecs::World,
        entity_pos: Vector2D,
        fields_masks: &mut HashMap<RoomId, HashMap<PlayerKey, u64>>,
    ) -> Option<PlayerAnchors> {
        let room_masks = fields_masks.entry(room_id).or_default();
        match policy.spatial {
            SpatialFilter::Global => {
                for &pk in players {
                    *room_masks.entry(pk).or_default() |= policy.fields_mask;
                }

                None
            }
            SpatialFilter::Radius(radius) => Some(self.merge_mask_within_area(
                room_id,
                players,
                policy.fields_mask,
                world,
                RadialArea {
                    position: entity_pos,
                    radius,
                },
                room_masks,
            )),
            SpatialFilter::Area(area) => Some(self.merge_mask_within_area(
                room_id,
                players,
                policy.fields_mask,
                world,
                area,
                room_masks,
            )),
        }
    }

    // Replicate changes
    pub fn replicate(&self, lua: &mlua::Lua, tick: u64) -> eyre::Result<()> {
        while let Ok(mark) = self.mark_rx.try_recv() {
            self.mark_update(mark);
        }

        let lost_anchors = {
            let inner = self.inner.borrow();

            let world =
                get_app_data::<app_data::World>(lua).wrap_err("App data is not initialized")?;
            let mut field_registry =
                get_app_data_mut::<FieldRegistry>(lua).wrap_err("App data is not initialized")?;

            let mut snapshots: HashMap<RoomId, HashMap<PlayerKey, WorldSnapshot>> = HashMap::new();
            let mut lost_anchors: HashMap<PlayerKey, HashSet<hecs::Entity>> = HashMap::new();
            let entity_customs = get_app_data::<app_data::EntityCustoms>(lua)
                .wrap_err("App data is not initialized")?;

            for (&entity, dirty_components) in inner.entities.iter() {
                let mut query = world.query_one::<(&Room, &Position, &Blueprint)>(entity);
                if let Ok(components) = query.get() {
                    let (room_comp, pos_comp, blueprint_comp) = components;

                    let room_id = room_comp.0;
                    let blueprint_id = blueprint_comp.0;
                    let position = pos_comp.0;

                    // If there are players in this room who need to receive updates
                    if let Some(room_players) = inner.room_to_players.get(&room_id) {
                        let mut fields_masks: HashMap<RoomId, HashMap<PlayerKey, u64>> =
                            HashMap::new();

                        let blueprint_policy_ids = inner
                            .by_target
                            .get(&ReplicationTarget::Blueprint(blueprint_id))
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
                                            room_players,
                                            policy,
                                            &*world,
                                            position,
                                            &mut fields_masks,
                                        );
                                    }
                                    PolicyRouting::Pinned(pinned_room_id) => {
                                        recently_lost_anchors = self.apply_spatial_filter_for_room(
                                            pinned_room_id,
                                            room_players,
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
                                let mut entity_data = EntityData::default();
                                for comp in dirty_components {
                                    match comp {
                                        EntityDirtyComponent::Core(comp) => {
                                            let key = ComponentKey::from(comp);
                                            let bit = field_registry.get_bit_index(key.as_ref())?;
                                            if (mask & (1 << bit)) == 0 {
                                                continue;
                                            }

                                            match comp {
                                                ComponentData::Name(name) => {
                                                    entity_data.name = Some(name.0.clone());
                                                }
                                                ComponentData::Position(_) => {
                                                    entity_data.position = Some(position);
                                                }
                                                ComponentData::Rotation(rotation) => {
                                                    entity_data.rotation = Some(rotation.0);
                                                }
                                                ComponentData::Control(control) => {
                                                    entity_data.speed = Some(control.speed);
                                                }
                                                ComponentData::Sprite2D(sprite_2d) => {
                                                    entity_data.sprite = Some(sprite_2d.0.clone());
                                                }
                                                ComponentData::SpriteChar(sprite_char) => {
                                                    entity_data.char = Some(sprite_char.0.clone());
                                                }
                                                ComponentData::OwnedBy(owned_by) => {
                                                    entity_data.owned_by = Some(owned_by.0);
                                                }
                                                ComponentData::Blueprint(_)
                                                | ComponentData::Room(_) => {}
                                            }
                                        }
                                        EntityDirtyComponent::Custom => {
                                            let entity_id = entity.id();

                                            let mut map: serde_json::Map<
                                                String,
                                                serde_json::Value,
                                            > = serde_json::Map::new();
                                            if let Some(custom) = entity_customs.get(&entity) {
                                                for pair in custom.pairs::<String, mlua::Value>() {
                                                    let (key, value) = pair.wrap_err(&format!("Failed to convert a custom table field to a needed type for an entity with ID '{}'", entity_id))?;
                                                    map.insert(key, lua.from_value(value).wrap_err(&format!("Failed to convert a custom table value to a needed type for an entity with ID '{}'", entity_id))?);
                                                }
                                            }

                                            entity_data.custom = Some(map);
                                        }
                                    }
                                }

                                snapshot.entities.insert(entity.id(), entity_data);
                            }
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
