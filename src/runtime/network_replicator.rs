use color_eyre::eyre;
use mlua::{IntoLuaMulti, LuaSerdeExt};
use rock_wire::{EntityData, OutgoingPacket, PlayerKey, WorldSnapshot, components::RadialArea};
use rustc_hash::FxHashMap;
use slotmap::SlotMap;
use smallvec::smallvec;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    str::FromStr,
    sync::Arc,
};

pub mod protocol;
use protocol::*;

mod field_registry;
pub(crate) use field_registry::FieldRegistry;

use crate::{
    envelope::{EnvelopeRecipient, ServerEnvelope},
    runtime::{
        GameModeClientApi, LuaResultExt, app_data, get_app_data, get_app_data_mut,
        plugins::{
            entity::components::{
                Blueprint, ComponentData, ComponentKey, Control, Name, OwnedBy, Position, Room,
                Rotation, Sprite2D, SpriteChar,
            },
            on::protocol::{EventScope, GameModeEvent, GameModeEventData, PlayerEventData},
            player::PlayerHandle,
        },
    },
    rx::{RxSentry, RxSentryError, core::CoreSentryError},
    utils::{custom_table_to_json, multivalue_to_json},
};

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
struct PlayerAnchor {
    pk: PlayerKey,
    entity: hecs::Entity,
}

struct NetworkReplicatorInner {
    updated_entities: FxHashMap<hecs::Entity, Vec<EntityDirtyComponent>>,
    memory_snapshots: FxHashMap<String, serde_json::Value>,

    policies: SlotMap<PolicyId, ReplicationPolicy>,
    by_target: FxHashMap<ReplicationTarget, HashSet<PolicyId>>,

    sentries: FxHashMap<ReplicationTarget, FxHashMap<PolicyId, RxSentry>>,

    // Pinned policies only
    room_to_policies: FxHashMap<RoomId, HashSet<PolicyId>>,

    player_to_rooms: FxHashMap<PlayerKey, HashSet<RoomId>>,
    room_to_players: FxHashMap<RoomId, HashSet<PlayerKey>>,

    player_anchors: FxHashMap<PlayerKey, HashSet<hecs::Entity>>,
    room_to_anchors: FxHashMap<RoomId, HashSet<PlayerAnchor>>,

    entity_anchors: FxHashMap<hecs::Entity, HashSet<PlayerAnchor>>,
    anchor_visibility: FxHashMap<PlayerAnchor, HashSet<hecs::Entity>>,
    known_memory: FxHashMap<PlayerKey, HashSet<String>>,

    despawn_candidates: FxHashMap<hecs::Entity, RoomId>,
    room_to_entities: FxHashMap<RoomId, HashSet<hecs::Entity>>,
}

pub(crate) struct NetworkReplicator {
    client_api: Arc<dyn GameModeClientApi>,
    inner: RefCell<NetworkReplicatorInner>,
    mark_tx: flume::Sender<ReplicationMark>,
    mark_rx: flume::Receiver<ReplicationMark>,
}
impl NetworkReplicator {
    pub fn new(client_api: Arc<dyn GameModeClientApi>) -> Self {
        let (mark_tx, mark_rx) = flume::unbounded::<ReplicationMark>();

        Self {
            client_api,
            inner: RefCell::new(NetworkReplicatorInner {
                updated_entities: FxHashMap::default(),
                memory_snapshots: FxHashMap::default(),
                policies: SlotMap::<PolicyId, ReplicationPolicy>::with_key(),
                by_target: FxHashMap::default(),
                sentries: FxHashMap::default(),
                room_to_policies: FxHashMap::default(),
                player_to_rooms: FxHashMap::default(),
                room_to_players: FxHashMap::default(),
                player_anchors: FxHashMap::default(),
                room_to_anchors: FxHashMap::default(),
                entity_anchors: FxHashMap::default(),
                anchor_visibility: FxHashMap::default(),
                known_memory: FxHashMap::default(),
                despawn_candidates: FxHashMap::default(),
                room_to_entities: FxHashMap::default(),
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
            ReplicationMark::Entity { entity, action } => {
                match action {
                    EntityReplicationAction::Spawn(room_id) => {
                        inner
                            .room_to_entities
                            .entry(room_id)
                            .or_default()
                            .insert(entity);
                    }
                    EntityReplicationAction::Update(comp) => match comp {
                        EntityDirtyComponent::Core(ComponentData::Room(_)) => {
                            // WARNING: :room() method doesn't trigger a replication action (unlike other component methods)
                            //          just in case if it was triggered and we are here -> we just ignore it
                            //          because clients shouldn't receive updates related to the internal logic of network replicator
                            //
                            // TODO:    maybe it'll be better to get rid of two sources of truth about entity room (hecs and network replicator)
                        }
                        _ => {
                            inner.updated_entities.entry(entity).or_default().push(comp);
                        }
                    },
                    EntityReplicationAction::Warp { from, to } => {
                        let anchor_owner = self.get_anchor_owner(&entity, &inner.player_anchors);

                        if let Some(old_room_id) = from {
                            if let Some(entities) = inner.room_to_entities.get_mut(&old_room_id) {
                                entities.remove(&entity);
                            }

                            if anchor_owner.is_some() {
                                inner
                                    .room_to_anchors
                                    .entry(old_room_id)
                                    .and_modify(|anchors| {
                                        anchors.retain(|anchor| anchor.entity != entity)
                                    });
                            }

                            inner.despawn_candidates.insert(entity, old_room_id);
                        }

                        if let Some(new_room_id) = to {
                            inner
                                .room_to_entities
                                .entry(new_room_id)
                                .or_default()
                                .insert(entity);

                            if let Some(pk) = anchor_owner {
                                inner
                                    .room_to_anchors
                                    .entry(new_room_id)
                                    .or_default()
                                    .insert(PlayerAnchor { pk, entity });
                            }

                            inner.updated_entities.entry(entity).or_default();
                        }
                    }
                    EntityReplicationAction::Despawn(room_id) => {
                        if let Some(entities) = inner.room_to_entities.get_mut(&room_id) {
                            entities.remove(&entity);
                        }

                        inner.despawn_candidates.insert(entity, room_id);
                    }
                }
            }
            ReplicationMark::Memory { key, value } => {
                inner.memory_snapshots.insert(key, value);
            }
        }
    }

    fn get_anchors_in_area(
        &self,
        world: &hecs::World,
        room_id: RoomId,
        area: RadialArea,
    ) -> Vec<PlayerAnchor> {
        let inner = self.inner.borrow();
        if let Some(anchors) = inner.room_to_anchors.get(&room_id) {
            anchors
                .iter()
                .filter(|anchor| self.visible_to_anchor(room_id, area, anchor.entity, world))
                .cloned()
                .collect()
        } else {
            Vec::new()
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
        self.get_anchors_in_area(world, room_id, area)
            .iter()
            .map(|anchor| anchor.pk)
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
    pub fn revoke_policies_by_target(&self, target: &ReplicationTarget) {
        let mut inner_guard = self.inner.borrow_mut();
        let NetworkReplicatorInner {
            policies,
            by_target,
            sentries,
            room_to_policies,
            ..
        } = &mut *inner_guard;

        if let Some(policy_ids) = by_target.get_mut(target) {
            for &policy_id in policy_ids.iter() {
                if let Some(policy) = policies.remove(policy_id)
                    && let PolicyRouting::Pinned(room_id) = policy.routing
                {
                    room_to_policies
                        .entry(room_id)
                        .and_modify(|ids| ids.retain(|&id| id != policy_id));
                }
            }
        }

        by_target.remove(target);
        sentries.remove(target);
    }

    pub fn commit_policy(&self, policy: ReplicationPolicy) -> eyre::Result<PolicyId> {
        let mut inner = self.inner.borrow_mut();

        match &policy.target {
            ReplicationTarget::MemoryNode(node) => match policy.routing {
                PolicyRouting::DynamicFollow => {
                    return Err(eyre::eyre!(
                        "Failed to commit a policy: memory node '{}' requires a target room",
                        node
                    ));
                }
                PolicyRouting::Pinned(room_id) => {
                    if let Some(policy_ids) = inner.by_target.get(&policy.target) {
                        for &policy_id in policy_ids {
                            let Some(existing_policy) = inner.policies.get(policy_id) else {
                                continue;
                            };

                            if existing_policy.routing == policy.routing {
                                return Err(eyre::eyre!(
                                    "Failed to commit a policy: collision occurred, memory node '{}' is already being replicated to room '{:?}'",
                                    node,
                                    room_id
                                ));
                            }
                        }
                    }
                }
            },
            ReplicationTarget::Blueprint(_) | ReplicationTarget::Entity(_) => {
                if matches!(policy.routing, PolicyRouting::Pinned(_)) {
                    return Err(eyre::eyre!(
                        "Failed to commit a policy: cannot pin a room for a policy with a blueprint or an entity target"
                    ));
                }
            }
        }

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

        Ok(id)
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
                    if matches!(filter, SpatialFilter::Radius(_)) {
                        match policy.target {
                            ReplicationTarget::Blueprint(_) | ReplicationTarget::Entity(_) => {}
                            _ => {
                                return Err(eyre::eyre!(
                                    "Failed to update policy with ID '{:?}': policy cannot have a radius-based spatial filter if a target is not an entity",
                                    updated_id
                                ));
                            }
                        }
                    }

                    policy.spatial = filter;
                }
                PolicyFieldUpdate::Room { id: new_id } => {
                    match policy.target {
                        ReplicationTarget::Blueprint(_) | ReplicationTarget::Entity(_) => {
                            return Err(eyre::eyre!(
                                "Failed to update policy with ID '{:?}': cannot change the room for a policy if a policy has a blueprint or an entity target",
                                updated_id
                            ));
                        }
                        _ => {}
                    }

                    match policy.routing {
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
                    }
                }
            }
        }

        Ok(())
    }

    pub fn add_player_anchor(&self, pk: PlayerKey, entity: hecs::Entity, room_id: Option<RoomId>) {
        let mut inner = self.inner.borrow_mut();
        inner.player_anchors.entry(pk).or_default().insert(entity);

        if let Some(room_id) = room_id {
            inner
                .room_to_anchors
                .entry(room_id)
                .or_default()
                .insert(PlayerAnchor { pk, entity });
        }
    }
    pub fn remove_player_anchor(
        &self,
        pk: PlayerKey,
        entity: hecs::Entity,
        room_id: Option<RoomId>,
    ) {
        let mut inner = self.inner.borrow_mut();
        if let Some(anchors) = inner.player_anchors.get_mut(&pk) {
            anchors.remove(&entity);
        }

        if let Some(room_id) = room_id {
            inner
                .room_to_anchors
                .entry(room_id)
                .and_modify(|anchors| anchors.retain(|anchor| anchor.entity != entity));
        }
    }
    pub fn clear_player_anchors(&self, lua: &mlua::Lua, pk: PlayerKey) -> mlua::Result<()> {
        let mut inner = self.inner.borrow_mut();
        if let Some(anchors) = inner.player_anchors.remove(&pk) {
            let world = get_app_data::<app_data::World>(lua)?;
            let mut cleared_anchors_per_room: HashMap<RoomId, HashSet<hecs::Entity>> =
                HashMap::new();
            for anchor in anchors {
                if let Ok(room_comp) = world.0.get::<&Room>(anchor) {
                    cleared_anchors_per_room
                        .entry(room_comp.0)
                        .or_default()
                        .insert(anchor);
                }
            }

            for (room_id, cleared_anchors) in cleared_anchors_per_room {
                inner.room_to_anchors.entry(room_id).and_modify(|anchors| {
                    anchors.retain(|anchor| !cleared_anchors.contains(&anchor.entity))
                });
            }
        }

        Ok(())
    }

    fn get_anchor_owner(
        &self,
        entity: &hecs::Entity,
        player_anchors: &FxHashMap<PlayerKey, HashSet<hecs::Entity>>,
    ) -> Option<PlayerKey> {
        for (&pk, anchors) in player_anchors {
            if anchors.contains(entity) {
                return Some(pk);
            }
        }

        None
    }
    fn visible_to_anchor(
        &self,
        room_id: RoomId,
        area: RadialArea,
        anchor: hecs::Entity,
        world: &hecs::World,
    ) -> bool {
        let mut query = world.query_one::<(&Room, &Position)>(anchor);
        if let Ok((room_comp, pos_comp)) = query.get() {
            pos_comp.0.distance_squared(&area.position) <= area.radius * area.radius
                && room_comp.0 == room_id
        } else {
            false
        }
    }

    pub fn add_player_to_room(
        &self,
        lua: &mlua::Lua,
        pk: PlayerKey,
        id: RoomId,
    ) -> mlua::Result<()> {
        let mut inner = self.inner.borrow_mut();

        inner.player_to_rooms.entry(pk).or_default().insert(id);
        inner.room_to_players.entry(id).or_default().insert(pk);

        let event_bus = get_app_data::<app_data::EventBus>(lua)?;
        event_bus.0.schedule_event(GameModeEvent {
            scopes: smallvec![EventScope::Global],
            data: GameModeEventData::Player(PlayerEventData::Enter {
                player: PlayerHandle::new(pk),
                room: id,
            }),
        });

        Ok(())
    }
    pub fn remove_player_from_room(
        &self,
        lua: &mlua::Lua,
        pk: PlayerKey,
        id: RoomId,
    ) -> mlua::Result<()> {
        let mut inner = self.inner.borrow_mut();

        inner
            .player_to_rooms
            .entry(pk)
            .and_modify(|rooms| rooms.retain(|&r_id| r_id != id));
        inner
            .room_to_players
            .entry(id)
            .and_modify(|pks| pks.retain(|&r_pk| r_pk != pk));

        let event_bus = get_app_data::<app_data::EventBus>(lua)?;
        event_bus.0.schedule_event(GameModeEvent {
            scopes: smallvec![EventScope::Global],
            data: GameModeEventData::Player(PlayerEventData::Exit {
                player: PlayerHandle::new(pk),
                room: id,
            }),
        });

        Ok(())
    }
    pub fn clear_player_rooms(&self, lua: &mlua::Lua, pk: PlayerKey) -> mlua::Result<()> {
        let mut inner = self.inner.borrow_mut();

        if let Some(rooms) = inner.player_to_rooms.remove(&pk) {
            let event_bus = get_app_data::<app_data::EventBus>(lua)?;
            let player_handle = PlayerHandle::new(pk);

            for room_id in rooms {
                event_bus.0.schedule_event(GameModeEvent {
                    scopes: smallvec![EventScope::Global],
                    data: GameModeEventData::Player(PlayerEventData::Exit {
                        player: player_handle.clone(),
                        room: room_id,
                    }),
                });

                inner
                    .room_to_players
                    .entry(room_id)
                    .and_modify(|pks| pks.retain(|&r_pk| r_pk != pk));
            }
        }

        Ok(())
    }

    fn compose_dirty_entity_data(
        &self,
        lua: &mlua::Lua,
        mask: &u64,
        entity: &hecs::Entity,
        dirty_components: &[EntityDirtyComponent],
        field_registry: &mut FieldRegistry,
        entity_customs: &app_data::EntityCustoms,
    ) -> eyre::Result<EntityData> {
        let mut entity_data = EntityData::default();
        for comp in dirty_components {
            match comp {
                EntityDirtyComponent::Core(comp) => {
                    let key = ComponentKey::from(comp);
                    let key_str = key.as_ref();

                    let bit = field_registry.get_bit_index(key_str).ok_or_else(|| {
                        eyre::eyre!("Failed to get a bit index for key '{}'", key_str)
                    })?;
                    if (mask & (1 << bit)) == 0 {
                        continue;
                    }

                    match comp {
                        ComponentData::Name(name) => {
                            entity_data.name = Some(name.0.clone());
                        }
                        ComponentData::Position(position) => {
                            entity_data.position = Some(position.0);
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
                        ComponentData::Blueprint(_) | ComponentData::Room(_) => {}
                    }
                }
                EntityDirtyComponent::Custom => {
                    // FIXME: one custom field change triggers a whole custom component replication
                    entity_data.custom = custom_table_to_json(lua, entity_customs.0.get(entity)).wrap_err(&format!(
                        "Failed to convert a custom component table to JSON for an entity with ID '{}'",
                        entity.id()
                    ))?;
                }
            }
        }

        Ok(entity_data)
    }
    fn apply_mask_on_entity_data(
        &self,
        entity: hecs::Entity,
        data: &mut EntityData,
        custom_data: &serde_json::Map<String, serde_json::Value>,
        mut mask: u64,
        field_registry: &FieldRegistry,
        world: &hecs::World,
    ) -> eyre::Result<()> {
        while mask != 0 {
            let bit = mask.trailing_zeros() as u8;

            if let Some(field_name) = field_registry.get_field_name(bit) {
                match ComponentKey::from_str(field_name) {
                    Ok(comp_key) => match comp_key {
                        ComponentKey::Position => {
                            if let Ok(pos_comp) = world.get::<&Position>(entity) {
                                data.position = Some(pos_comp.0);
                            }
                        }
                        ComponentKey::Rotation => {
                            if let Ok(rotation_comp) = world.get::<&Rotation>(entity) {
                                data.rotation = Some(rotation_comp.0);
                            }
                        }
                        ComponentKey::Control => {
                            if let Ok(control_comp) = world.get::<&Control>(entity) {
                                data.speed = Some(control_comp.speed);
                            }
                        }
                        ComponentKey::Sprite2D => {
                            if let Ok(sprite_2d_comp) = world.get::<&Sprite2D>(entity) {
                                data.sprite = Some(sprite_2d_comp.0.clone());
                            }
                        }
                        ComponentKey::SpriteChar => {
                            if let Ok(sprite_char_comp) = world.get::<&SpriteChar>(entity) {
                                data.char = Some(sprite_char_comp.0.clone());
                            }
                        }
                        ComponentKey::OwnedBy => {
                            if let Ok(owned_by_comp) = world.get::<&OwnedBy>(entity) {
                                data.owned_by = Some(owned_by_comp.0);
                            }
                        }
                        ComponentKey::Name => {
                            if let Ok(name_comp) = world.get::<&Name>(entity) {
                                data.name = Some(name_comp.0.clone());
                            }
                        }
                        ComponentKey::Blueprint | ComponentKey::Room => {}
                    },
                    Err(_) => {
                        if let Some(custom_value) = custom_data.get(field_name) {
                            data.custom
                                .insert(field_name.to_string(), custom_value.clone());
                        }
                    }
                }
            }

            mask &= mask - 1;
        }

        Ok(())
    }
    fn append_entity_data(&self, from: &EntityData, to: &mut EntityData) {
        to.name = from.name.clone().or(to.name.take());
        to.speed = from.speed.or(to.speed.take());
        to.owned_by = from.owned_by.or(to.owned_by.take());
        to.sprite = from.sprite.clone().or(to.sprite.take());
        to.char = from.char.clone().or(to.char.take());
        to.position = from.position.or(to.position.take());
        to.rotation = from.rotation.or(to.rotation.take());
        to.custom.extend(from.custom.clone());
    }

    fn process_despawned_entities(
        &self,
        snapshots: &mut HashMap<PlayerKey, WorldSnapshot>,
        base_snapshot: &WorldSnapshot,
    ) {
        let mut inner_guard = self.inner.borrow_mut();
        let NetworkReplicatorInner {
            updated_entities,
            entity_anchors,
            anchor_visibility,
            despawn_candidates,
            ..
        } = &mut *inner_guard;

        let despawn_candidates: FxHashMap<hecs::Entity, RoomId> =
            std::mem::take(despawn_candidates);

        for &entity in despawn_candidates.keys() {
            updated_entities.remove(&entity);

            if let Some(anchors) = entity_anchors.remove(&entity) {
                let entity_id = entity.id();
                for anchor in &anchors {
                    let snapshot = snapshots
                        .entry(anchor.pk)
                        .or_insert_with(|| base_snapshot.clone());
                    if !snapshot.despawn.contains(&entity_id) {
                        snapshot.despawn.push(entity_id);
                    }

                    let empty = if let Some(entities) = anchor_visibility.get_mut(anchor) {
                        entities.remove(&entity);
                        entities.is_empty()
                    } else {
                        false
                    };
                    if empty {
                        anchor_visibility.remove(anchor);
                    }
                }
            }
        }
        drop(inner_guard);

        for &entity in despawn_candidates.keys() {
            self.revoke_policies_by_target(&ReplicationTarget::Entity(entity));
        }
    }
    fn process_entities(
        &self,
        lua: &mlua::Lua,
        snapshots: &mut HashMap<PlayerKey, WorldSnapshot>,
        base_snapshot: &WorldSnapshot,
    ) -> eyre::Result<HashSet<PolicyId>> {
        let mut inner_guard = self.inner.borrow_mut();
        let NetworkReplicatorInner {
            updated_entities,
            policies,
            by_target,
            sentries,
            player_anchors,
            room_to_anchors,
            entity_anchors,
            anchor_visibility,
            room_to_entities,
            ..
        } = &mut *inner_guard;

        let world_data =
            get_app_data::<app_data::World>(lua).wrap_err("App data is not initialized")?;
        let world = &world_data.0;
        let entities_view = world.view::<(Option<&Blueprint>, Option<&Position>)>();

        let mut field_registry =
            get_app_data_mut::<FieldRegistry>(lua).wrap_err("App data is not initialized")?;
        let entity_customs =
            get_app_data::<app_data::EntityCustoms>(lua).wrap_err("App data is not initialized")?;

        let mut policies_to_remove: HashSet<PolicyId> = HashSet::new();
        for (&room_id, entities) in room_to_entities {
            for &entity in entities.iter() {
                let Some((blueprint_comp, pos_comp)) = entities_view.get(entity) else {
                    continue;
                };

                let entity_id = entity.id();
                let entity_custom_data = custom_table_to_json(lua, entity_customs.0.get(&entity))
                    .wrap_err(&format!(
                    "Failed to deserialize a custom data table for an entity with ID '{}'",
                    entity_id
                ))?;

                let blueprint_id = blueprint_comp.map(|c| c.0);
                let position = pos_comp.map(|c| c.0);

                let blueprint_policy_ids = blueprint_id
                    .and_then(|b_id| by_target.get(&ReplicationTarget::Blueprint(b_id)))
                    .into_iter()
                    .flatten();
                let entity_policy_ids = by_target
                    .get(&ReplicationTarget::Entity(entity))
                    .into_iter()
                    .flatten();
                let policy_ids = blueprint_policy_ids
                    .chain(entity_policy_ids)
                    .copied()
                    .collect::<Vec<_>>();

                let dirty_components = updated_entities.get(&entity);
                let entity_sentries = sentries
                    .entry(ReplicationTarget::Entity(entity))
                    .or_default();

                let entity_attached_anchors = entity_anchors.entry(entity).or_default();
                let current_room_anchors = room_to_anchors.entry(room_id).or_default();

                let mut spawn_for: HashMap<PolicyId, HashSet<PlayerKey>> = HashMap::new();
                let mut update_for: HashMap<PolicyId, HashSet<PlayerKey>> = HashMap::new();
                let mut despawn_for: HashMap<PlayerKey, bool> = HashMap::new();

                entity_attached_anchors.retain(|anchor| {
                    let same_room = world
                        .get::<&Room>(anchor.entity)
                        .is_ok_and(|room| room.0 == room_id);
                    let alive = world.contains(anchor.entity);
                    let attached = same_room && alive;

                    if !attached {
                        despawn_for.insert(anchor.pk, true);

                        if let Some(visibility) = anchor_visibility.get_mut(anchor) {
                            visibility.remove(&entity);
                        }
                    }
                    if !alive {
                        current_room_anchors.remove(anchor);
                        if let Some(anchors) = player_anchors.get_mut(&anchor.pk) {
                            anchors.remove(&anchor.entity);
                        }
                    }

                    attached
                });

                for &policy_id in &policy_ids {
                    let Some(policy) = policies.get(policy_id) else {
                        continue;
                    };

                    for anchor in current_room_anchors.iter() {
                        let visible = match policy.spatial {
                            SpatialFilter::Global => true,
                            SpatialFilter::Radius(radius) => position.is_some_and(|position| {
                                self.visible_to_anchor(
                                    room_id,
                                    RadialArea { position, radius },
                                    anchor.entity,
                                    world,
                                )
                            }),
                            SpatialFilter::Area(area) => {
                                self.visible_to_anchor(room_id, area, anchor.entity, world)
                            }
                        };
                        let known = anchor_visibility
                            .get(anchor)
                            .is_some_and(|entities| entities.contains(&entity));
                        let needs_update = known && dirty_components.is_some();

                        if visible {
                            despawn_for.insert(anchor.pk, false);

                            if !known {
                                spawn_for.entry(policy_id).or_default().insert(anchor.pk);

                                // Attach anchor to entity
                                entity_attached_anchors.insert(anchor.clone());
                                anchor_visibility
                                    .entry(anchor.clone())
                                    .or_default()
                                    .insert(entity);
                            } else if needs_update {
                                update_for.entry(policy_id).or_default().insert(anchor.pk);
                            }
                        } else if known {
                            despawn_for.entry(anchor.pk).or_insert(true);

                            entity_attached_anchors.remove(anchor);
                            if let Some(visibility) = anchor_visibility.get_mut(anchor) {
                                visibility.remove(&entity);
                            }
                        }
                    }
                }

                for &policy_id in &policy_ids {
                    let Some(policy) = policies.get(policy_id) else {
                        continue;
                    };

                    if let Some(spawn_recipients) = spawn_for.get(&policy_id) {
                        for &pk in spawn_recipients {
                            let snapshot =
                                snapshots.entry(pk).or_insert_with(|| base_snapshot.clone());

                            let entity_data = snapshot
                                .rooms
                                .entry(room_id)
                                .or_default()
                                .spawn
                                .entry(entity_id)
                                .or_default();
                            self.apply_mask_on_entity_data(
                                entity,
                                entity_data,
                                &entity_custom_data,
                                policy.fields_mask,
                                &field_registry,
                                world,
                            )?;
                        }
                    }

                    if let Some(update_recipients) = update_for.get(&policy_id)
                        && let Some(components) = dirty_components
                    {
                        if update_recipients.is_empty() {
                            continue;
                        }

                        let sentry = entity_sentries
                            .entry(policy_id)
                            .or_insert_with(|| RxSentry::new(policy.pipeline.clone()));
                        match sentry.process(
                            ().into_lua_multi(lua)
                                .wrap_err("Failed to convert an empty value `()` to Lua")?,
                        ) {
                            Ok(_) => {
                                let updates = self.compose_dirty_entity_data(
                                    lua,
                                    &policy.fields_mask,
                                    &entity,
                                    components,
                                    &mut field_registry,
                                    &entity_customs,
                                )?;

                                for &pk in update_recipients {
                                    let snapshot = snapshots
                                        .entry(pk)
                                        .or_insert_with(|| base_snapshot.clone());
                                    self.append_entity_data(
                                        &updates,
                                        snapshot
                                            .rooms
                                            .entry(room_id)
                                            .or_default()
                                            .update
                                            .entry(entity_id)
                                            .or_default(),
                                    );
                                }
                            }
                            Err(err) => match err {
                                RxSentryError::Core(CoreSentryError::LimitReached(_)) => {
                                    if !matches!(policy.target, ReplicationTarget::Blueprint(_)) {
                                        policies_to_remove.insert(policy_id);
                                    }
                                }
                                RxSentryError::Core(CoreSentryError::Skipping)
                                | RxSentryError::Core(CoreSentryError::Throttled) => {}
                                RxSentryError::Op(err) => {
                                    return Err(eyre::eyre!(
                                        "Failed to process Rx sentry for entity with ID '{}': operator error ({})",
                                        entity_id,
                                        err.to_string()
                                    ));
                                }
                            },
                        }
                    }
                }

                for (pk, despawn_candidate) in despawn_for {
                    if !despawn_candidate {
                        continue;
                    }

                    let still_attached =
                        entity_attached_anchors.iter().any(|anchor| anchor.pk == pk);
                    if still_attached {
                        continue;
                    }

                    let snapshot = snapshots.entry(pk).or_insert_with(|| base_snapshot.clone());
                    if !snapshot.despawn.contains(&entity_id) {
                        snapshot.despawn.push(entity_id);
                    }
                }
            }
        }

        Ok(policies_to_remove)
    }
    fn process_memory_nodes(
        &self,
        lua: &mlua::Lua,
        snapshots: &mut HashMap<PlayerKey, WorldSnapshot>,
        base_snapshot: &WorldSnapshot,
    ) -> eyre::Result<HashSet<PolicyId>> {
        let mut inner_guard = self.inner.borrow_mut();
        let NetworkReplicatorInner {
            memory_snapshots,
            policies,
            by_target,
            sentries,
            room_to_anchors,
            known_memory,
            ..
        } = &mut *inner_guard;

        let world = get_app_data::<app_data::World>(lua).wrap_err("App data is not initialized")?;

        let mut policies_to_remove: HashSet<PolicyId> = HashSet::new();
        for (key, snapshot_value) in memory_snapshots.iter() {
            let target = ReplicationTarget::MemoryNode(key.clone());
            let Some(policy_ids) = by_target.get(&target) else {
                continue;
            };

            if policy_ids.is_empty() {
                continue;
            }

            let node_sentries = sentries.entry(target).or_default();

            let lua_snapshot_value = lua.to_value(snapshot_value).wrap_err(&format!(
                "Failed to convert JSON value for memory node '{}' to Lua value",
                key
            ))?;
            let node_args = mlua::MultiValue::from_vec(vec![lua_snapshot_value]);

            for &policy_id in policy_ids {
                let Some(policy) = policies.get(policy_id) else {
                    continue;
                };

                let policy_room_id = match policy.routing {
                    PolicyRouting::DynamicFollow => {
                        // Ignore a memory node policy with a dynamic follow routing rule (this is normally unreachable, but anyway)
                        continue;
                    }
                    PolicyRouting::Pinned(pinned_room_id) => pinned_room_id,
                };

                let Some(anchors) = room_to_anchors.get(&policy_room_id) else {
                    continue;
                };

                // List of affected anchors without a boolean indicator (memory nodes don't need an update, or a state diff)
                let mut affected_anchors: Vec<&PlayerAnchor> = Vec::new();
                for anchor in anchors {
                    let known_nodes = known_memory.entry(anchor.pk).or_default();
                    if known_nodes.contains(key) {
                        continue;
                    }

                    let affected = match policy.spatial {
                        SpatialFilter::Global => true,
                        SpatialFilter::Radius(_) => {
                            // Ignore a memory node policy with a radius-based spatial filter (this is normally unreachable, but anyway)
                            false
                        }
                        SpatialFilter::Area(area) => {
                            self.visible_to_anchor(policy_room_id, area, anchor.entity, &world.0)
                        }
                    };

                    if !affected {
                        continue;
                    }

                    affected_anchors.push(anchor);
                }

                if affected_anchors.is_empty() {
                    continue;
                }

                let sentry = node_sentries
                    .entry(policy_id)
                    .or_insert_with(|| RxSentry::new(policy.pipeline.clone()));
                match sentry.process(node_args.clone()) {
                    Ok(args) => {
                        let Some(args) = args else { continue };
                        let json_str = multivalue_to_json(lua, args).wrap_err(&format!(
                            "Failed to convert processed sentry args to JSON for memory node, key '{}'",
                            key
                        ))?;

                        for anchor in affected_anchors {
                            snapshots
                                .entry(anchor.pk)
                                .or_insert_with(|| base_snapshot.clone())
                                .rooms
                                .entry(policy_room_id)
                                .or_default()
                                .state
                                .insert(key.clone(), json_str.clone());

                            // TODO: known memory is independent of spatial filters (not garbage collected by the client if out of visibility range)
                            known_memory
                                .entry(anchor.pk)
                                .or_default()
                                .insert(key.clone());
                        }
                    }
                    Err(err) => {
                        match err {
                            RxSentryError::Core(CoreSentryError::LimitReached(_)) => {
                                policies_to_remove.insert(policy_id);
                            }
                            RxSentryError::Core(CoreSentryError::Skipping)
                            | RxSentryError::Core(CoreSentryError::Throttled) => {}
                            RxSentryError::Op(err) => {
                                return Err(eyre::eyre!(
                                    "Failed to process Rx sentry for memory node with key '{}': operator error ({})",
                                    key,
                                    err.to_string()
                                ));
                            }
                        }

                        continue;
                    }
                }
            }
        }

        Ok(policies_to_remove)
    }
    fn build_snapshots(
        &self,
        lua: &mlua::Lua,
        tick: u64,
    ) -> eyre::Result<HashMap<PlayerKey, WorldSnapshot>> {
        let mut snapshots: HashMap<PlayerKey, WorldSnapshot> = HashMap::new();
        let base_snapshot = WorldSnapshot::new(tick);

        self.process_despawned_entities(&mut snapshots, &base_snapshot);

        let mut policies_to_remove = HashSet::new();
        policies_to_remove.extend(self.process_entities(lua, &mut snapshots, &base_snapshot)?);
        policies_to_remove.extend(self.process_memory_nodes(
            lua,
            &mut snapshots,
            &base_snapshot,
        )?);

        // Remove taken policies
        for policy_id in policies_to_remove {
            self.revoke_policy_by_id(policy_id);
        }

        Ok(snapshots)
    }

    // Replicate changes
    pub fn replicate(&self, lua: &mlua::Lua, tick: u64) -> eyre::Result<()> {
        // Mark updates
        while let Ok(mark) = self.mark_rx.try_recv() {
            self.mark_update(mark);
        }

        // Build snapshots
        let snapshots = self.build_snapshots(lua, tick)?;

        // Cleanup
        let mut inner = self.inner.borrow_mut();
        inner.updated_entities.clear();

        // Send snapshots
        for (pk, snapshot) in snapshots {
            self.client_api.send(ServerEnvelope {
                recipient: EnvelopeRecipient::Single(pk),
                payload: OutgoingPacket::World(snapshot),
            });
        }

        Ok(())
    }
}
