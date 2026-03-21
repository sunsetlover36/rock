use color_eyre::eyre;
use mlua::{IntoLuaMulti, LuaSerdeExt};
use rustc_hash::FxHashMap;
use shared::{
    EntityData, OutgoingPacket, PlayerKey, WorldSnapshot,
    components::{RadialArea, Vector2D},
};
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
    updated_memory: FxHashMap<String, serde_json::Value>,

    policies: SlotMap<PolicyId, ReplicationPolicy>,
    by_target: FxHashMap<ReplicationTarget, HashSet<PolicyId>>,

    sentries: FxHashMap<ReplicationTarget, FxHashMap<PolicyId, RxSentry>>,

    // Pinned policies only
    room_to_policies: FxHashMap<RoomId, HashSet<PolicyId>>,

    player_to_rooms: FxHashMap<PlayerKey, HashSet<RoomId>>,
    room_to_players: FxHashMap<RoomId, HashSet<PlayerKey>>,
    player_anchors: FxHashMap<PlayerKey, HashSet<hecs::Entity>>,
    room_to_anchors: FxHashMap<RoomId, HashSet<PlayerAnchor>>,

    known_entities: FxHashMap<PlayerKey, HashSet<hecs::Entity>>,
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
                updated_memory: FxHashMap::default(),
                policies: SlotMap::<PolicyId, ReplicationPolicy>::with_key(),
                by_target: FxHashMap::default(),
                sentries: FxHashMap::default(),
                room_to_policies: FxHashMap::default(),
                player_to_rooms: FxHashMap::default(),
                room_to_players: FxHashMap::default(),
                player_anchors: FxHashMap::default(),
                room_to_anchors: FxHashMap::default(),
                known_entities: FxHashMap::default(),
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
            ReplicationMark::Entity { entity, action } => match action {
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
                    let anchor_owner = self.get_anchor_owner(&entity);

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

                    if let Some(pk) = self.get_anchor_owner(&entity) {
                        self.remove_player_anchor(pk, entity, Some(room_id));
                    }

                    inner.despawn_candidates.insert(entity, room_id);
                }
            },
            ReplicationMark::Memory { key, value } => {
                inner.updated_memory.insert(key, value);
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
                if let Some(policy) = policies.remove(policy_id) {
                    if let PolicyRouting::Pinned(room_id) = policy.routing {
                        room_to_policies
                            .entry(room_id)
                            .and_modify(|ids| ids.retain(|&id| id != policy_id));
                    }
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
            ReplicationTarget::Entity(_) => {
                if matches!(policy.routing, PolicyRouting::Pinned(_))
                    && matches!(policy.spatial, SpatialFilter::Radius(_))
                {
                    return Err(eyre::eyre!(
                        "Failed to commit a policy: policy with a pinned room routing cannot have a radius-based spatial filter"
                    ));
                }
            }
            _ => {}
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
                if let Ok(room_comp) = world.get::<&Room>(anchor) {
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

    fn get_anchor_owner(&self, entity: &hecs::Entity) -> Option<PlayerKey> {
        let inner = self.inner.borrow();
        for (&pk, anchors) in &inner.player_anchors {
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
    fn visible_to_anchors(
        &self,
        room_id: RoomId,
        area: RadialArea,
        anchors: &HashSet<hecs::Entity>,
        world: &hecs::World,
    ) -> bool {
        anchors
            .iter()
            .any(|&anchor| self.visible_to_anchor(room_id, area, anchor, world))
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
        event_bus.schedule_event(GameModeEvent {
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
        event_bus.schedule_event(GameModeEvent {
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
                event_bus.schedule_event(GameModeEvent {
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

        inner.known_entities.remove(&pk);

        Ok(())
    }

    fn merge_mask_within_area(
        &self,
        room_id: RoomId,
        players: &HashSet<PlayerKey>,
        mask: u64,
        world: &hecs::World,
        area: RadialArea,
        room_masks: &mut FxHashMap<PlayerKey, u64>,
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
        fields_masks: &mut FxHashMap<RoomId, FxHashMap<PlayerKey, u64>>,
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

    fn compose_entity_data(
        &self,
        lua: &mlua::Lua,
        world: &hecs::World,
        entity: hecs::Entity,
    ) -> eyre::Result<EntityData> {
        let entity_customs =
            get_app_data::<app_data::EntityCustoms>(lua).wrap_err("App data is not initialized")?;

        let mut query = world.query_one::<(
            Option<&Name>,
            Option<&Control>,
            Option<&OwnedBy>,
            Option<&Sprite2D>,
            Option<&SpriteChar>,
            Option<&Position>,
            Option<&Rotation>,
        )>(entity);

        let (
            name_comp,
            control_comp,
            owned_by_comp,
            sprite_2d_comp,
            sprite_char_comp,
            position_comp,
            rotation_comp,
        ) = query.get()?;
        let custom = custom_table_to_json(lua, entity_customs.get(&entity)).wrap_err(&format!(
            "Failed to convert a custom component table to JSON for an entity with ID '{}'",
            entity.id()
        ))?;

        let entity_data = EntityData {
            name: name_comp.map(|c| c.0.clone()),
            speed: control_comp.map(|c| c.speed),
            owned_by: owned_by_comp.map(|c| c.0),
            sprite: sprite_2d_comp.map(|c| c.0.clone()),
            char: sprite_char_comp.map(|c| c.0.clone()),
            position: position_comp.map(|c| c.0),
            rotation: rotation_comp.map(|c| c.0),
            custom,
        };
        Ok(entity_data)
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
                    let bit = field_registry.get_bit_index(key.as_ref())?;
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
                    entity_data.custom = custom_table_to_json(lua, entity_customs.get(&entity)).wrap_err(&format!(
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
        mut mask: u64,
        field_registry: &FieldRegistry,
        world: &hecs::World,
    ) -> eyre::Result<()> {
        while mask != 0 {
            let bit = mask.trailing_zeros() as u8;

            if let Some(field_name) = field_registry.get_field_name(bit) {
                let comp_key = ComponentKey::from_str(field_name)?;
                match comp_key {
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
        to.position = from.position.clone().or(to.position.take());
        to.rotation = from.rotation.clone().or(to.rotation.take());
        to.custom.extend(from.custom.clone());
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
    ) -> eyre::Result<(FxHashMap<hecs::Entity, Vec<PolicyId>>, Vec<PolicyId>)> {
        let world = get_app_data::<app_data::World>(lua).wrap_err("App data is not initialized")?;

        let mut inner_guard = self.inner.borrow_mut();
        let NetworkReplicatorInner {
            updated_entities,
            policies,
            by_target,
            sentries,
            room_to_anchors,
            ..
        } = &mut *inner_guard;

        // Remove despawned anchors
        for anchors in room_to_anchors.values_mut() {
            anchors.retain(|anchor| world.contains(anchor.entity));
        }

        let mut allowed_policies_per_entity: FxHashMap<hecs::Entity, Vec<PolicyId>> =
            FxHashMap::default();
        let mut policies_to_remove: Vec<PolicyId> = Vec::new();

        for (&entity, _) in updated_entities.iter() {
            if let Ok(blueprint_comp) = world.get::<&Blueprint>(entity) {
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
        snapshots: &mut FxHashMap<PlayerKey, WorldSnapshot>,
    ) -> eyre::Result<()> {
        let (allowed_policies_per_entity, entity_policies_to_remove) =
            self.process_entity_sentries(lua)?;

        let world = get_app_data::<app_data::World>(lua).wrap_err("App data is not initialized")?;
        let mut field_registry =
            get_app_data_mut::<FieldRegistry>(lua).wrap_err("App data is not initialized")?;
        let entity_customs =
            get_app_data::<app_data::EntityCustoms>(lua).wrap_err("App data is not initialized")?;

        let inner = self.inner.borrow();
        let mut newly_discovered_entities: FxHashMap<PlayerKey, HashSet<hecs::Entity>> =
            FxHashMap::default();

        for (&entity, dirty_components) in inner.updated_entities.iter() {
            let mut query = world.query_one::<(&Room, &Position, Option<&Blueprint>)>(entity);
            if let Ok(components) = query.get() {
                let (room_comp, pos_comp, blueprint_comp) = components;

                let room_id = room_comp.0;
                let blueprint_id = blueprint_comp.map(|c| c.0);
                let position = pos_comp.0;

                // If there are players in this room who need to receive updates
                if let Some(room_players) = inner.room_to_players.get(&room_id) {
                    let mut fields_masks: FxHashMap<RoomId, FxHashMap<PlayerKey, u64>> =
                        FxHashMap::default();

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
                                .or_insert_with(|| WorldSnapshot::new(tick))
                                .rooms
                                .entry(room_id)
                                .or_default();
                            let known_to_player = inner
                                .known_entities
                                .get(&pk)
                                .map_or(false, |set| set.contains(&entity));

                            let entity_id = entity.id();
                            if known_to_player {
                                room_snapshot.update.insert(
                                    entity_id,
                                    self.compose_dirty_entity_data(
                                        lua,
                                        mask,
                                        &entity,
                                        dirty_components,
                                        &mut *field_registry,
                                        &*entity_customs,
                                    )?,
                                );
                            } else {
                                room_snapshot.spawn.insert(
                                    entity_id,
                                    self.compose_entity_data(lua, &*world, entity)?,
                                );
                                newly_discovered_entities
                                    .entry(pk)
                                    .or_default()
                                    .insert(entity);
                            }
                        }
                    }
                }
            }
        }
        drop(inner);

        let mut inner = self.inner.borrow_mut();
        for (&pk, entities) in newly_discovered_entities.iter() {
            inner.known_entities.entry(pk).or_default().extend(entities);
        }
        drop(inner);

        // Remove taken policies
        for policy_id in entity_policies_to_remove {
            self.revoke_policy_by_id(policy_id);
        }

        Ok(())
    }

    // Process memory nodes
    fn process_memory_nodes(
        &self,
        lua: &mlua::Lua,
        tick: u64,
        snapshots: &mut FxHashMap<PlayerKey, WorldSnapshot>,
    ) -> eyre::Result<()> {
        let mut inner_guard = self.inner.borrow_mut();
        let NetworkReplicatorInner {
            updated_memory,
            policies,
            by_target,
            sentries,
            ..
        } = &mut *inner_guard;

        let mut policies_to_remove: Vec<PolicyId> = Vec::new();
        for (key, value) in updated_memory.iter() {
            let key_str: Arc<str> = Arc::from(key.as_str());

            let target = ReplicationTarget::MemoryNode(key.clone());
            let Some(policy_ids) = by_target.get(&target) else {
                continue;
            };
            let node_sentries = sentries.entry(target).or_default();

            let lua_value = lua.to_value(&value).wrap_err(&format!(
                "Failed to convert JSON value for memory node '{}' to Lua value",
                key
            ))?;

            for &policy_id in policy_ids {
                let Some(policy) = policies.get(policy_id) else {
                    continue;
                };

                match policy.routing {
                    PolicyRouting::DynamicFollow => {
                        return Err(eyre::eyre!(
                            "Failed to process a memory node policy: encountered a memory node policy with dynamic follow routing, key {}",
                            key
                        ));
                    }
                    PolicyRouting::Pinned(room_id) => {
                        let sentry = node_sentries
                            .entry(policy_id)
                            .or_insert_with(|| RxSentry::new(policy.pipeline.clone()));

                        let args = mlua::MultiValue::from_vec(vec![lua_value.clone()]);
                        match sentry.process(args) {
                            Ok(Some(args)) => {
                                let json_str: Arc<str> =
                                                Arc::from(multivalue_to_json(lua, args).wrap_err(&format!("Failed to convert processed sentry args to JSON for memory node, key '{}'", key))?);

                                let mut write_snapshot = |pk| {
                                    let room_snapshot = snapshots
                                        .entry(pk)
                                        .or_insert_with(|| WorldSnapshot::new(tick))
                                        .rooms
                                        .entry(room_id)
                                        .or_default();
                                    room_snapshot
                                        .state
                                        .insert(key_str.clone(), json_str.clone());
                                };
                                match policy.spatial {
                                    SpatialFilter::Global => {
                                        for pk in self.get_players_in_room(room_id) {
                                            write_snapshot(pk);
                                        }
                                    }
                                    SpatialFilter::Radius(_) => {
                                        return Err(eyre::eyre!(
                                            "Failed to process a memory node policy: encountered a memory node policy with radius-based spatial filter, key {}",
                                            key
                                        ));
                                    }
                                    SpatialFilter::Area(area) => {
                                        let world = get_app_data::<app_data::World>(lua)
                                            .wrap_err("App data is not initialized")?;
                                        for anchor in
                                            self.get_anchors_in_area(&*world, room_id, area)
                                        {
                                            write_snapshot(anchor.pk);
                                        }
                                    }
                                }
                            }
                            Ok(None) => {}
                            Err(err) => match err {
                                RxSentryError::Core(CoreSentryError::LimitReached(_)) => {
                                    policies_to_remove.push(policy_id);
                                }
                                RxSentryError::Core(CoreSentryError::Skipping)
                                | RxSentryError::Core(CoreSentryError::Throttled) => {}
                                RxSentryError::Op(err) => {
                                    return Err(eyre::eyre!(
                                        "Failed to process Rx sentry for memory node '{}': operator error ({})",
                                        key,
                                        err.to_string()
                                    ));
                                }
                            },
                        }
                    }
                }
            }
        }
        drop(inner_guard);

        for policy_id in policies_to_remove {
            self.revoke_policy_by_id(policy_id);
        }

        Ok(())
    }
    fn process_despawned_entities(
        &self,
        tick: u64,
        snapshots: &mut FxHashMap<PlayerKey, WorldSnapshot>,
    ) {
        let mut inner_guard = self.inner.borrow_mut();
        let NetworkReplicatorInner {
            known_entities,
            despawn_candidates,
            ..
        } = &mut *inner_guard;

        let despawn_candidates: FxHashMap<hecs::Entity, RoomId> =
            despawn_candidates.drain().collect();

        for (&pk, entities) in known_entities {
            entities.retain(|e| {
                despawn_candidates.get(e).map_or(true, |_| {
                    snapshots
                        .entry(pk)
                        .or_insert_with(|| WorldSnapshot::new(tick))
                        .despawn
                        .push(e.id());

                    false
                })
            });
        }
        drop(inner_guard);

        for (&entity, _) in despawn_candidates.iter() {
            self.revoke_policies_by_target(&ReplicationTarget::Entity(entity));
        }
    }
    fn despawn_by_spatial(
        &self,
        lua: &mlua::Lua,
        tick: u64,
        snapshots: &mut FxHashMap<PlayerKey, WorldSnapshot>,
    ) -> eyre::Result<()> {
        let world = get_app_data::<app_data::World>(lua).wrap_err("App data is not initialized")?;

        let mut inner_guard = self.inner.borrow_mut();
        let NetworkReplicatorInner {
            policies,
            by_target,
            player_anchors,
            known_entities,
            ..
        } = &mut *inner_guard;

        for (&pk, entities) in known_entities {
            let Some(anchors) = player_anchors.get(&pk) else {
                continue;
            };

            let mut despawned_entities: HashSet<hecs::Entity> = HashSet::new();
            for &entity in entities.iter() {
                let mut mark_despawned = || {
                    snapshots
                        .entry(pk)
                        .or_insert_with(|| WorldSnapshot::new(tick))
                        .despawn
                        .push(entity.id());
                    despawned_entities.insert(entity);
                };

                let mut query = world.query_one::<(Option<&Blueprint>, &Room, &Position)>(entity);
                match query.get() {
                    Ok(components) => {
                        let (blueprint_comp, room_comp, pos_comp) = components;
                        let blueprint_id = blueprint_comp.map(|c| c.0);
                        let room_id = room_comp.0;
                        let position = pos_comp.0;

                        let blueprint_policy_ids = blueprint_id
                            .and_then(|id| by_target.get(&ReplicationTarget::Blueprint(id)))
                            .into_iter()
                            .flatten();
                        let entity_policy_ids = by_target
                            .get(&ReplicationTarget::Entity(entity))
                            .into_iter()
                            .flatten();
                        let mut policy_ids = blueprint_policy_ids.chain(entity_policy_ids);

                        let visible = policy_ids.any(|&policy_id| {
                            let Some(policy) = policies.get(policy_id) else {
                                return false;
                            };

                            match policy.spatial {
                                SpatialFilter::Global => true,
                                SpatialFilter::Radius(radius) => self.visible_to_anchors(
                                    room_id,
                                    RadialArea { position, radius },
                                    anchors,
                                    &*world,
                                ),
                                SpatialFilter::Area(area) => {
                                    self.visible_to_anchors(room_id, area, anchors, &*world)
                                }
                            }
                        });

                        if !visible {
                            mark_despawned();
                        }
                    }
                    Err(_) => {
                        mark_despawned();
                    }
                }
            }

            entities.retain(|e| !despawned_entities.contains(e));
        }

        Ok(())
    }

    // Replicate changes
    pub fn replicate(&self, lua: &mlua::Lua, tick: u64) -> eyre::Result<()> {
        while let Ok(mark) = self.mark_rx.try_recv() {
            self.mark_update(mark);
        }

        // Construct snapshots
        let mut snapshots: FxHashMap<PlayerKey, WorldSnapshot> = FxHashMap::default();
        let base_snapshot = WorldSnapshot::new(tick);

        let mut inner_guard = self.inner.borrow_mut();
        let NetworkReplicatorInner {
            updated_entities,
            policies,
            by_target,
            sentries,
            room_to_anchors,
            known_entities,
            room_to_entities,
            ..
        } = &mut *inner_guard;

        let world = get_app_data::<app_data::World>(lua).wrap_err("App data is not initialized")?;
        let mut field_registry =
            get_app_data_mut::<FieldRegistry>(lua).wrap_err("App data is not initialized")?;
        let entity_customs =
            get_app_data::<app_data::EntityCustoms>(lua).wrap_err("App data is not initialized")?;

        // remove despawned anchors
        // <->

        for (&room_id, entities) in room_to_entities {
            for &entity in entities.iter() {
                let mut query = world.query_one::<(&Blueprint, Option<&Position>)>(entity);
                let Ok((blueprint_comp, pos_comp)) = query.get() else {
                    continue;
                };

                let blueprint_id = blueprint_comp.0;
                let position = pos_comp.map(|pos_comp| pos_comp.0);

                let blueprint_policy_ids = by_target
                    .get(&ReplicationTarget::Blueprint(blueprint_id))
                    .into_iter()
                    .flatten();
                let entity_policy_ids = by_target
                    .get(&ReplicationTarget::Entity(entity))
                    .into_iter()
                    .flatten();
                let policy_ids = blueprint_policy_ids.chain(entity_policy_ids);

                let dirty_components = updated_entities.get(&entity);
                let entity_sentries = sentries
                    .entry(ReplicationTarget::Entity(entity))
                    .or_default();

                let mut policies_to_remove: Vec<PolicyId> = Vec::new();
                for &policy_id in policy_ids {
                    if let Some(policy) = policies.get(policy_id) {
                        let sentry = entity_sentries
                            .entry(policy_id)
                            .or_insert_with(|| RxSentry::new(policy.pipeline.clone()));

                        let policy_room_id = match policy.routing {
                            PolicyRouting::DynamicFollow => room_id,
                            PolicyRouting::Pinned(pinned_room_id) => pinned_room_id,
                        };

                        if let Some(anchors) = room_to_anchors.get(&policy_room_id) {
                            for anchor in anchors {
                                let affected = match policy.spatial {
                                    SpatialFilter::Global => true,
                                    SpatialFilter::Radius(radius) => {
                                        // Ignore a policy with a pinned room routing rule and a radius-based spatial filter (this is normally unreachable, but anyway)
                                        matches!(policy.routing, PolicyRouting::DynamicFollow)
                                            && position.map_or(false, |position| {
                                                self.visible_to_anchor(
                                                    room_id,
                                                    RadialArea { position, radius },
                                                    anchor.entity,
                                                    &*world,
                                                )
                                            })
                                    }
                                    SpatialFilter::Area(area) => self.visible_to_anchor(
                                        room_id,
                                        area,
                                        anchor.entity,
                                        &*world,
                                    ),
                                };

                                if !affected {
                                    continue;
                                }

                                let snapshot = snapshots
                                    .entry(anchor.pk)
                                    .or_insert_with(|| base_snapshot.clone());
                                let known = known_entities
                                    .get(&anchor.pk)
                                    .map_or(false, |entities| entities.contains(&entity));
                                let needs_update = known && dirty_components.is_some();

                                // process sentry if:
                                // 1. entity is unknown -> first replication on spawn
                                // 2. entity is known and was updated -> replication on update
                                if !known || needs_update {
                                    if let Err(err) =
                                        sentry.process(().into_lua_multi(lua).wrap_err(
                                            "Failed to convert an empty value `()` to Lua",
                                        )?)
                                    {
                                        match err {
                                            RxSentryError::Core(CoreSentryError::LimitReached(
                                                _,
                                            )) => {
                                                if !matches!(
                                                    policy.target,
                                                    ReplicationTarget::Blueprint(_)
                                                ) {
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
                                        }

                                        continue;
                                    }
                                }

                                if needs_update {
                                    if let Some(components) = dirty_components {
                                        let updates = self.compose_dirty_entity_data(
                                            lua,
                                            &policy.fields_mask,
                                            &entity,
                                            components,
                                            &mut *field_registry,
                                            &*entity_customs,
                                        )?;
                                        self.append_entity_data(
                                            &updates,
                                            snapshot
                                                .rooms
                                                .entry(policy_room_id)
                                                .or_default()
                                                .update
                                                .entry(entity.id())
                                                .or_default(),
                                        );
                                    }
                                } else if !known {
                                    let entity_data = snapshot
                                        .rooms
                                        .entry(policy_room_id)
                                        .or_default()
                                        .spawn
                                        .entry(entity.id())
                                        .or_default();
                                    self.apply_mask_on_entity_data(
                                        entity,
                                        entity_data,
                                        policy.fields_mask,
                                        &*field_registry,
                                        &*world,
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
        }
        drop(inner_guard);

        self.process_despawned_entities(tick, &mut snapshots);
        self.process_entities(lua, tick, &mut snapshots)?;
        self.process_memory_nodes(lua, tick, &mut snapshots)?;
        self.despawn_by_spatial(lua, tick, &mut snapshots)?;

        // Cleanup
        let mut inner = self.inner.borrow_mut();
        inner.updated_entities.clear();
        inner.updated_memory.clear();

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
