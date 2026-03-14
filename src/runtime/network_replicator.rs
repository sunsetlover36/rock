use color_eyre::eyre;
use mlua::{IntoLuaMulti, LuaSerdeExt};
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

use crate::{
    runtime::{
        GameModeClientApi, LuaResultExt, app_data, get_app_data, get_app_data_mut,
        plugins::entity::components::{Blueprint, ComponentData, ComponentKey, Position, Room},
    },
    rx::{RxSentry, RxSentryError, core::CoreSentryError},
};

type PlayerAnchors = HashMap<PlayerKey, HashSet<hecs::Entity>>;

struct NetworkReplicatorInner {
    client_api: Arc<dyn GameModeClientApi>,

    entities: HashMap<hecs::Entity, Vec<EntityDirtyComponent>>,
    memory: HashMap<String, serde_json::Value>,

    policies: SlotMap<PolicyId, ReplicationPolicy>,
    by_target: HashMap<ReplicationTarget, HashSet<PolicyId>>,

    sentries: HashMap<ReplicationTarget, HashMap<PolicyId, RxSentry>>,

    // Pinned policies only
    room_to_policies: HashMap<RoomId, HashSet<PolicyId>>,

    player_anchors: PlayerAnchors,
    player_to_rooms: HashMap<PlayerKey, HashSet<RoomId>>,
    room_to_players: HashMap<RoomId, HashSet<PlayerKey>>,

    entities_snapshots: HashMap<PlayerKey, HashMap<RoomId, HashSet<hecs::Entity>>>,
    // memory_snapshots?
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
                sentries: HashMap::new(),
                room_to_policies: HashMap::new(),
                player_to_rooms: HashMap::new(),
                room_to_players: HashMap::new(),
                player_anchors: HashMap::new(),
                entities_snapshots: HashMap::new(),
                client_api,
            }),
            mark_tx,
            mark_rx,
        }
    }

    pub fn get_mark_tx(&self) -> flume::Sender<ReplicationMark> {
        self.mark_tx.clone()
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

    pub fn get_players_in_room(&self, room_id: RoomId) -> Vec<PlayerKey> {
        self.inner
            .borrow()
            .room_to_players
            .get(&room_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect()
    }
    pub fn get_players_in_area(
        &self,
        world: &hecs::World,
        room_id: RoomId,
        area: RadialArea,
    ) -> Vec<PlayerKey> {
        let players_in_room = self.get_players_in_room(room_id).into_iter();

        let inner = self.inner.borrow();
        players_in_room
            .filter(|pk| {
                if let Some(anchors) = inner.player_anchors.get(pk) {
                    self.visible_to_anchors(room_id, area, anchors, world)
                } else {
                    false
                }
            })
            .collect()
    }

    pub fn revoke_policy_by_id(&self, revoked_id: PolicyId) {
        let mut inner = self.inner.borrow_mut();
        if let Some(policy) = inner.policies.remove(revoked_id) {
            let target = policy.target;
            inner
                .by_target
                .entry(target.clone())
                .and_modify(|policies| policies.retain(|&id| id != revoked_id));
            inner.sentries.entry(target.clone()).and_modify(|policies| {
                policies.remove(&revoked_id);
            });

            if let PolicyRouting::Pinned(room_id) = policy.routing {
                inner
                    .room_to_policies
                    .entry(room_id)
                    .and_modify(|ids| ids.retain(|&id| id != revoked_id));
            }
        }
    }
    pub fn revoke_policies_by_target(&self, target: ReplicationTarget) {
        let mut inner_guard = self.inner.borrow_mut();
        let NetworkReplicatorInner {
            policies,
            by_target,
            sentries,
            room_to_policies,
            ..
        } = &mut *inner_guard;

        if let Some(policy_ids) = by_target.get_mut(&target) {
            for &policy_id in policy_ids.iter() {
                if let Some(policy) = policies.remove(policy_id) {
                    if let PolicyRouting::Pinned(room_id) = policy.routing {
                        room_to_policies
                            .entry(room_id)
                            .and_modify(|ids| ids.retain(|&id| id != policy_id));
                    }
                }
            }
        }

        by_target.remove(&target);
        sentries.remove(&target);
    }

    pub fn commit_policy(&self, policy: ReplicationPolicy) -> PolicyId {
        let mut inner = self.inner.borrow_mut();
        let target = policy.target.clone();
        let routing = policy.routing;

        let id = inner.policies.insert(policy);
        inner.by_target.entry(target).or_default().insert(id);

        if let PolicyRouting::Pinned(room_id) = routing {
            inner
                .room_to_policies
                .entry(room_id)
                .or_default()
                .insert(id);
        }

        id
    }
    pub fn update_policy(
        &self,
        updated_id: PolicyId,
        field: PolicyFieldUpdate,
    ) -> eyre::Result<()> {
        let NetworkReplicatorInner {
            policies,
            room_to_policies,
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

                        room_to_policies
                            .entry(old_id)
                            .and_modify(|ids| ids.retain(|&id| id != updated_id));
                        room_to_policies
                            .entry(new_id)
                            .or_default()
                            .insert(updated_id);

                        policy.routing = PolicyRouting::Pinned(new_id);
                    }
                },
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
                            .room_to_policies
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
    fn visible_to_anchors(
        &self,
        room_id: RoomId,
        area: RadialArea,
        anchors: &HashSet<hecs::Entity>,
        world: &hecs::World,
    ) -> bool {
        let radius_sq = area.radius * area.radius;
        anchors.iter().any(|&anchor| {
            let mut query = world.query_one::<(&Room, &Position)>(anchor);
            if let Ok((room_comp, pos_comp)) = query.get() {
                return room_comp.0 == room_id
                    && area.position.distance_squared(&pos_comp.0) <= radius_sq;
            }

            false
        })
    }

    pub fn add_player_to_room(&self, pk: PlayerKey, id: RoomId) {
        let mut inner = self.inner.borrow_mut();

        inner.player_to_rooms.entry(pk).or_default().insert(id);
        inner.room_to_players.entry(id).or_default().insert(pk);
        inner
            .entities_snapshots
            .entry(pk)
            .or_default()
            .insert(id, HashSet::new());
    }
    pub fn remove_player_from_room(&self, pk: PlayerKey, id: RoomId) {
        let mut inner = self.inner.borrow_mut();

        inner
            .player_to_rooms
            .entry(pk)
            .and_modify(|rooms| rooms.retain(|&r_id| r_id != id));
        inner
            .room_to_players
            .entry(id)
            .and_modify(|pks| pks.retain(|&r_pk| r_pk != pk));
        inner.entities_snapshots.entry(pk).and_modify(|rooms| {
            rooms.remove(&id);
        });
    }
    pub fn clear_player_rooms(&self, pk: PlayerKey) {
        let mut inner = self.inner.borrow_mut();

        if let Some(rooms) = inner.player_to_rooms.remove(&pk) {
            for room_id in rooms {
                inner
                    .room_to_players
                    .entry(room_id)
                    .and_modify(|pks| pks.retain(|&r_pk| r_pk != pk));
            }
        }

        inner.entities_snapshots.remove(&pk);
    }

    fn merge_mask_within_area(
        &self,
        room_id: RoomId,
        players: &HashSet<PlayerKey>,
        mask: u64,
        world: &hecs::World,
        area: RadialArea,
        room_masks: &mut HashMap<PlayerKey, u64>,
    ) {
        let inner = self.inner.borrow();

        for &pk in players {
            if let Some(anchors) = inner.player_anchors.get(&pk) {
                if self.visible_to_anchors(room_id, area, anchors, world) {
                    *room_masks.entry(pk).or_default() |= mask;
                }
            }
        }
    }

    // Applies a spatial filter for the selected room using a policy fields mask
    fn apply_spatial_filter_for_room(
        &self,
        room_id: RoomId,
        players: &HashSet<PlayerKey>,
        policy: &ReplicationPolicy,
        world: &hecs::World,
        entity_pos: Vector2D,
        fields_masks: &mut HashMap<RoomId, HashMap<PlayerKey, u64>>,
    ) {
        let room_masks = fields_masks.entry(room_id).or_default();
        match policy.spatial {
            SpatialFilter::Global => {
                for &pk in players {
                    *room_masks.entry(pk).or_default() |= policy.fields_mask;
                }
            }
            SpatialFilter::Radius(radius) => self.merge_mask_within_area(
                room_id,
                players,
                policy.fields_mask,
                world,
                RadialArea {
                    position: entity_pos,
                    radius,
                },
                room_masks,
            ),
            SpatialFilter::Area(area) => self.merge_mask_within_area(
                room_id,
                players,
                policy.fields_mask,
                world,
                area,
                room_masks,
            ),
        }
    }

    // -- Process entity sentries
    // Cleans up despawned player anchors (entities)
    // Returns:
    // 1. Allowed policies per updated entity for this tick
    // 2. Fully processed sentries that must be removed (:take limit reached)
    // --
    fn process_entity_sentries(
        &self,
        lua: &mlua::Lua,
    ) -> eyre::Result<(HashMap<hecs::Entity, Vec<PolicyId>>, Vec<PolicyId>)> {
        let world = get_app_data::<app_data::World>(lua).wrap_err("App data is not initialized")?;

        let mut inner_guard = self.inner.borrow_mut();
        let NetworkReplicatorInner {
            entities,
            policies,
            by_target,
            sentries,
            player_anchors,
            ..
        } = &mut *inner_guard;

        // Remove despawned anchors
        for anchors in player_anchors.values_mut() {
            anchors.retain(|&anchor| world.contains(anchor));
        }

        let mut allowed_policies_per_entity: HashMap<hecs::Entity, Vec<PolicyId>> = HashMap::new();
        let mut policies_to_remove: Vec<PolicyId> = Vec::new();

        for (&entity, _) in entities.iter() {
            if let Ok(blueprint_comp) = world.get::<&Blueprint>(entity.clone()) {
                let blueprint_id = blueprint_comp.0;
                let blueprint_policy_ids = by_target
                    .get(&ReplicationTarget::Blueprint(blueprint_id))
                    .into_iter()
                    .flatten();
                let entity_policy_ids = by_target
                    .get(&ReplicationTarget::Entity(entity))
                    .into_iter()
                    .flatten();

                for &policy_id in blueprint_policy_ids.chain(entity_policy_ids) {
                    if let Some(policy) = policies.get_mut(policy_id) {
                        let sentry = sentries
                            .entry(ReplicationTarget::Entity(entity))
                            .or_default()
                            .entry(policy_id)
                            .or_insert_with(|| RxSentry::new(policy.pipeline.clone()));

                        match sentry.process(
                            ().into_lua_multi(lua)
                                .wrap_err("Failed to convert an empty value `()` to Lua")?,
                        ) {
                            Ok(_) => {
                                allowed_policies_per_entity
                                    .entry(entity)
                                    .or_default()
                                    .push(policy_id);
                            }
                            Err(err) => match err {
                                RxSentryError::Core(CoreSentryError::LimitReached(_)) => {
                                    // No need to remove a blueprint policy
                                    // Remove policy for entity or memory node only
                                    if !matches!(policy.target, ReplicationTarget::Blueprint(_)) {
                                        policies_to_remove.push(policy_id);
                                    }
                                }
                                RxSentryError::Core(CoreSentryError::Skipping)
                                | RxSentryError::Core(CoreSentryError::Throttled) => {}
                                RxSentryError::Op(err) => {
                                    return Err(eyre::eyre!(
                                        "Failed to process Rx sentry for entity with ID '{}': operator error ({})",
                                        entity.id(),
                                        err.to_string()
                                    ));
                                }
                            },
                        }
                    }
                }
            }
        }

        Ok((allowed_policies_per_entity, policies_to_remove))
    }

    // -- Process changes for entities
    // Mutates a snapshot for each player based on entity replication mark
    // --
    fn process_entities(
        &self,
        lua: &mlua::Lua,
        tick: u64,
        snapshots: &mut HashMap<PlayerKey, WorldSnapshot>,
    ) -> eyre::Result<()> {
        let (allowed_policies_per_entity, entity_policies_to_remove) =
            self.process_entity_sentries(lua)?;

        {
            let world =
                get_app_data::<app_data::World>(lua).wrap_err("App data is not initialized")?;
            let mut field_registry =
                get_app_data_mut::<FieldRegistry>(lua).wrap_err("App data is not initialized")?;
            let entity_customs = get_app_data::<app_data::EntityCustoms>(lua)
                .wrap_err("App data is not initialized")?;

            let inner = self.inner.borrow();
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
                        let allowed_for_this_entity = allowed_policies_per_entity.get(&entity);
                        for &policy_id in policy_ids {
                            let is_allowed = allowed_for_this_entity
                                .map_or(false, |allowed| allowed.contains(&policy_id));
                            if is_allowed {
                                if let Some(policy) = inner.policies.get(policy_id) {
                                    match policy.routing {
                                        PolicyRouting::DynamicFollow => {
                                            self.apply_spatial_filter_for_room(
                                                room_id,
                                                room_players,
                                                policy,
                                                &*world,
                                                position,
                                                &mut fields_masks,
                                            );
                                        }
                                        PolicyRouting::Pinned(pinned_room_id) => {
                                            self.apply_spatial_filter_for_room(
                                                pinned_room_id,
                                                room_players,
                                                policy,
                                                &*world,
                                                position,
                                                &mut fields_masks,
                                            );
                                        }
                                    }
                                }
                            }
                        }

                        for (&room_id, masks) in fields_masks.iter() {
                            for (&pk, mask) in masks.iter() {
                                let room_snapshot = snapshots
                                    .entry(pk)
                                    .or_insert(WorldSnapshot::new(tick))
                                    .rooms
                                    .entry(room_id)
                                    .or_default();

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

                                room_snapshot.entities.insert(entity.id(), entity_data);
                            }
                        }
                    }
                }
            }
        }

        // Remove taken policies
        for policy_id in entity_policies_to_remove {
            self.revoke_policy_by_id(policy_id);
        }

        Ok(())
    }

    // Replicate changes
    pub fn replicate(&self, lua: &mlua::Lua, tick: u64) -> eyre::Result<()> {
        while let Ok(mark) = self.mark_rx.try_recv() {
            self.mark_update(mark);
        }

        let mut snapshots: HashMap<PlayerKey, WorldSnapshot> = HashMap::new();
        self.process_entities(lua, tick, &mut snapshots)?;

        // Cleanup
        {
            let mut inner = self.inner.borrow_mut();
            inner.entities.clear();
            inner.memory.clear();
        }

        Ok(())
    }
}
