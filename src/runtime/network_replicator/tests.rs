use super::*;
use crate::{
    rx::{RxPipeline, operator::RxOp},
    socket::protocol::ServerMessage,
};
use rock_wire::farcaster::Fid;

struct NoClients;
impl GameModeClientApi for NoClients {
    fn has(&self, _: PlayerKey) -> bool {
        false
    }
    fn list(&self) -> Vec<PlayerKey> {
        Vec::new()
    }
    fn send(&self, _: ServerMessage) {
        panic!("snapshot tests must not send packets")
    }
    fn kick(&self, _: PlayerKey) {}
    fn identity(&self, _: PlayerKey) -> Option<String> {
        None
    }
    fn fid(&self, _: PlayerKey) -> Option<Fid> {
        None
    }
}

struct Fixture {
    lua: mlua::Lua,
    replicator: std::rc::Rc<NetworkReplicator>,
    entity: hecs::Entity,
    player: PlayerKey,
}
impl Fixture {
    fn new() -> Self {
        let lua = mlua::Lua::new();
        let mut world = hecs::World::new();
        let position = Position(rock_wire::components::Position { x: 0.0, y: 0.0 });
        let entity = world.spawn((Blueprint(1), position, Room(1)));
        let anchor = world.spawn((position, Room(1)));
        lua.set_app_data(app_data::World(world));
        lua.set_app_data(app_data::EntityCustoms(HashMap::new()));
        lua.set_app_data(app_data::RoomIdToName(HashMap::from([(1, "world".into())])));
        lua.set_app_data(FieldRegistry::new(&lua).unwrap());
        let replicator = std::rc::Rc::new(NetworkReplicator::new(Arc::new(NoClients)));
        let player = PlayerKey {
            slot_idx: 0,
            generation: 0,
        };
        {
            let mut inner = replicator.inner.borrow_mut();
            inner.room_to_entities.entry(1).or_default().insert(entity);
            inner
                .room_to_anchors
                .entry(1)
                .or_default()
                .insert(PlayerAnchor {
                    pk: player,
                    entity: anchor,
                });
            inner
                .player_anchors
                .entry(player)
                .or_default()
                .insert(anchor);
        }
        Self {
            lua,
            replicator,
            entity,
            player,
        }
    }

    fn policy(&self, target: ReplicationTarget, operators: Vec<RxOp>) -> PolicyId {
        let mut policy = ReplicationPolicy::new(target);
        if matches!(policy.target, ReplicationTarget::MemoryNode(_)) {
            policy.routing = PolicyRouting::Pinned(1);
        }
        let mut pipeline = RxPipeline::default();
        for op in operators {
            pipeline.add_operator(op);
        }
        policy.pipeline = pipeline;
        self.replicator.commit_policy(policy).unwrap()
    }

    fn dirty(&self) {
        self.replicator.inner.borrow_mut().updated_entities.insert(
            self.entity,
            vec![EntityDirtyComponent::Core(ComponentData::Position(
                Position(rock_wire::components::Position { x: 3.0, y: 0.0 }),
            ))],
        );
    }

    fn snapshots(&self) -> HashMap<PlayerKey, WorldSnapshot> {
        self.replicator.build_snapshots(&self.lua, 1).unwrap()
    }

    fn has_update(&self, snapshots: &HashMap<PlayerKey, WorldSnapshot>) -> bool {
        snapshots
            .get(&self.player)
            .and_then(|s| s.rooms.get("world"))
            .is_some_and(|r| r.update.contains_key(&self.entity.id()))
    }
}

#[test]
fn entity_sync_does_not_send_skipped_updates() {
    let f = Fixture::new();
    f.policy(ReplicationTarget::Entity(f.entity), vec![RxOp::Skip(1)]);
    let initial = f.snapshots();
    assert!(
        initial[&f.player].rooms["world"]
            .spawn
            .contains_key(&f.entity.id())
    );
    f.dirty();
    assert!(!f.has_update(&f.snapshots()));
    assert!(f.has_update(&f.snapshots()));
}

#[test]
fn entity_sync_throttles_without_count_operators() {
    let f = Fixture::new();
    f.policy(
        ReplicationTarget::Blueprint(1),
        vec![RxOp::Throttle(std::time::Duration::from_secs(3600))],
    );
    f.snapshots();
    f.dirty();
    assert!(f.has_update(&f.snapshots()));
    assert!(!f.has_update(&f.snapshots()));
}

#[test]
fn memory_sync_filters_before_take_and_maps_payload() {
    let f = Fixture::new();
    let key = "shared".to_string();
    let filter = f.lua.create_function(|_, n: i64| Ok(n > 0)).unwrap();
    let map = f.lua.create_function(|_, n: i64| Ok(n * 10)).unwrap();
    f.policy(
        ReplicationTarget::MemoryNode(key.clone()),
        vec![RxOp::Filter(filter), RxOp::Take(1), RxOp::Map(map)],
    );
    f.replicator.mark_update(ReplicationMark::Memory {
        key: key.clone(),
        value: serde_json::json!(0),
    });
    assert!(f.snapshots().is_empty());
    f.replicator.mark_update(ReplicationMark::Memory {
        key: key.clone(),
        value: serde_json::json!(2),
    });
    let snapshots = f.snapshots();
    assert_eq!(snapshots[&f.player].rooms["world"].state[&key], "20");
    assert!(f.snapshots().is_empty());
}

impl Fixture {
    fn source(&self, key: &str, value: serde_json::Value) {
        self.replicator.mark_update(ReplicationMark::Memory {
            key: key.into(),
            value,
        });
    }
    fn anchor(&self) -> hecs::Entity {
        *self.replicator.inner.borrow().player_anchors[&self.player]
            .iter()
            .next()
            .unwrap()
    }
    fn move_anchor(&self, x: f32) {
        get_app_data::<app_data::World>(&self.lua)
            .unwrap()
            .0
            .get::<&mut Position>(self.anchor())
            .unwrap()
            .0
            .x = x;
    }
    fn area(&self, id: PolicyId) {
        self.replicator
            .update_policy(
                id,
                PolicyFieldUpdate::Spatial(SpatialPolicyUpdate::Set(SpatialFilter::Area(Area {
                    position: rock_wire::components::Position { x: 0.0, y: 0.0 },
                    radius: 10.0,
                    shape: AreaShape::Circle,
                }))),
            )
            .unwrap();
    }
}

#[test]
fn memory_updates_deduplicate_and_null_is_not_deletion() {
    let f = Fixture::new();
    f.policy(ReplicationTarget::MemoryNode("node".into()), vec![]);
    f.source("node", serde_json::json!(1));
    assert_eq!(f.snapshots()[&f.player].rooms["world"].state["node"], "1");
    f.source("node", serde_json::json!(1));
    assert!(f.snapshots().is_empty());
    f.source("node", serde_json::Value::Null);
    let snapshots = f.snapshots();
    assert_eq!(snapshots[&f.player].rooms["world"].state["node"], "null");
    assert!(snapshots[&f.player].rooms["world"].state_remove.is_empty());
    f.replicator
        .mark_update(ReplicationMark::MemoryDelete { key: "node".into() });
    assert_eq!(
        f.snapshots()[&f.player].rooms["world"].state_remove,
        ["node"]
    );
    assert!(f.snapshots().is_empty());
}

#[test]
fn memory_filter_withdraws_old_copy_and_skip_retains_it() {
    let f = Fixture::new();
    f.policy(
        ReplicationTarget::MemoryNode("filtered".into()),
        vec![RxOp::Filter(
            f.lua.create_function(|_, n: i64| Ok(n > 0)).unwrap(),
        )],
    );
    f.source("filtered", serde_json::json!(1));
    f.snapshots();
    f.source("filtered", serde_json::json!(-1));
    assert_eq!(
        f.snapshots()[&f.player].rooms["world"].state_remove,
        ["filtered"]
    );
    f.source("filtered", serde_json::json!(2));
    assert_eq!(
        f.snapshots()[&f.player].rooms["world"].state["filtered"],
        "2"
    );

    let mut p = ReplicationPolicy::new(ReplicationTarget::MemoryNode("skip".into()));
    p.pipeline.add_operator(RxOp::Skip(1));
    let mut publication = MemoryPublication::new(&p);
    publication.published = Some(Arc::from("old"));
    publication
        .update(
            &f.lua,
            Some(Arc::new(serde_json::json!(42))),
            std::time::Instant::now(),
        )
        .unwrap();
    assert_eq!(publication.published.as_deref(), Some("old"));
}

#[test]
fn memory_take_freezes_snapshot_but_visibility_and_revoke_still_apply() {
    let f = Fixture::new();
    let id = f.policy(
        ReplicationTarget::MemoryNode("node".into()),
        vec![RxOp::Take(1)],
    );
    f.area(id);
    f.source("node", serde_json::json!(1));
    f.snapshots();
    f.source("node", serde_json::json!(2));
    assert!(f.snapshots().is_empty());
    assert!(f.replicator.inner.borrow().policies.contains_key(id));
    f.move_anchor(30.0);
    assert_eq!(
        f.snapshots()[&f.player].rooms["world"].state_remove,
        ["node"]
    );
    f.move_anchor(0.0);
    assert_eq!(f.snapshots()[&f.player].rooms["world"].state["node"], "1");
    let newcomer = PlayerKey {
        slot_idx: 1,
        generation: 0,
    };
    let anchor = f.anchor();
    f.replicator
        .inner
        .borrow_mut()
        .room_to_anchors
        .get_mut(&1)
        .unwrap()
        .insert(PlayerAnchor {
            pk: newcomer,
            entity: anchor,
        });
    assert_eq!(f.snapshots()[&newcomer].rooms["world"].state["node"], "1");
    f.replicator.revoke_policy_by_id(id);
    let snapshots = f.snapshots();
    assert_eq!(snapshots[&f.player].rooms["world"].state_remove, ["node"]);
    assert_eq!(snapshots[&newcomer].rooms["world"].state_remove, ["node"]);
}

#[test]
fn memory_visibility_is_union_of_live_anchors_in_the_correct_room() {
    let f = Fixture::new();
    let id = f.policy(ReplicationTarget::MemoryNode("node".into()), vec![]);
    f.area(id);
    let second = get_app_data_mut::<app_data::World>(&f.lua)
        .unwrap()
        .0
        .spawn((
            Position(rock_wire::components::Position { x: 0.0, y: 0.0 }),
            Room(1),
        ));
    f.replicator
        .inner
        .borrow_mut()
        .room_to_anchors
        .get_mut(&1)
        .unwrap()
        .insert(PlayerAnchor {
            pk: f.player,
            entity: second,
        });
    f.source("node", serde_json::json!(1));
    f.snapshots();
    f.move_anchor(30.0);
    assert!(f.snapshots().is_empty());
    get_app_data_mut::<app_data::World>(&f.lua)
        .unwrap()
        .0
        .get::<&mut Room>(second)
        .unwrap()
        .0 = 2;
    assert_eq!(
        f.snapshots()[&f.player].rooms["world"].state_remove,
        ["node"]
    );
    get_app_data_mut::<app_data::World>(&f.lua)
        .unwrap()
        .0
        .despawn(second)
        .unwrap();
    assert!(f.snapshots().is_empty());
}

#[test]
fn memory_rerouting_moves_copy_without_replaying_take() {
    let f = Fixture::new();
    get_app_data_mut::<app_data::RoomIdToName>(&f.lua)
        .unwrap()
        .0
        .insert(2, "other".into());
    let anchor = f.anchor();
    f.replicator.inner.borrow_mut().room_to_anchors.insert(
        2,
        HashSet::from([PlayerAnchor {
            pk: f.player,
            entity: anchor,
        }]),
    );
    let id = f.policy(
        ReplicationTarget::MemoryNode("node".into()),
        vec![RxOp::Take(1)],
    );
    f.source("node", serde_json::json!(1));
    f.snapshots();
    f.replicator
        .update_policy(id, PolicyFieldUpdate::Room { id: 2 })
        .unwrap();
    get_app_data_mut::<app_data::World>(&f.lua)
        .unwrap()
        .0
        .get::<&mut Room>(f.anchor())
        .unwrap()
        .0 = 2;
    let snapshots = f.snapshots();
    assert_eq!(snapshots[&f.player].rooms["world"].state_remove, ["node"]);
    assert_eq!(snapshots[&f.player].rooms["other"].state["node"], "1");
}

#[test]
fn memory_same_key_has_independent_projection_per_room_and_reroute_rejects_collision() {
    let f = Fixture::new();
    get_app_data_mut::<app_data::RoomIdToName>(&f.lua)
        .unwrap()
        .0
        .insert(2, "other".into());
    let anchor = get_app_data_mut::<app_data::World>(&f.lua)
        .unwrap()
        .0
        .spawn((Room(2),));
    f.replicator.inner.borrow_mut().room_to_anchors.insert(
        2,
        HashSet::from([PlayerAnchor {
            pk: f.player,
            entity: anchor,
        }]),
    );
    let first = f.policy(ReplicationTarget::MemoryNode("node".into()), vec![]);
    let mut second = ReplicationPolicy::new(ReplicationTarget::MemoryNode("node".into()));
    second.routing = PolicyRouting::Pinned(2);
    second.pipeline.add_operator(RxOp::Map(
        f.lua.create_function(|_, n: i64| Ok(n * 10)).unwrap(),
    ));
    f.replicator.commit_policy(second).unwrap();
    f.source("node", serde_json::json!(3));
    let snapshots = f.snapshots();
    assert_eq!(snapshots[&f.player].rooms["world"].state["node"], "3");
    assert_eq!(snapshots[&f.player].rooms["other"].state["node"], "30");
    assert!(
        f.replicator
            .update_policy(first, PolicyFieldUpdate::Room { id: 2 })
            .is_err()
    );
    f.replicator.revoke_policy_by_id(first);
    let snapshots = f.snapshots();
    assert_eq!(snapshots[&f.player].rooms["world"].state_remove, ["node"]);
    assert!(!snapshots[&f.player].rooms.contains_key("other"));
}

#[test]
fn memory_throttle_coalesces_latest_and_resumes_without_replaying_prefix() {
    use std::time::{Duration, Instant};
    let lua = mlua::Lua::new();
    let mut policy = ReplicationPolicy::new(ReplicationTarget::MemoryNode("node".into()));
    policy.pipeline.add_operator(RxOp::Take(2));
    policy
        .pipeline
        .add_operator(RxOp::Throttle(Duration::from_secs(10)));
    let mut p = MemoryPublication::new(&policy);
    let t = Instant::now();
    p.update(&lua, Some(Arc::new(serde_json::json!(1))), t)
        .unwrap();
    let second = Arc::new(serde_json::json!(2));
    p.update(&lua, Some(second.clone()), t + Duration::from_secs(1))
        .unwrap();
    assert_eq!(p.published.as_deref(), Some("1"));
    // take(2) is exhausted, but its already accepted sample must finish.
    p.update(
        &lua,
        Some(Arc::new(serde_json::json!(3))),
        t + Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!(p.published.as_deref(), Some("2"));
    assert!(p.pending.is_none());

    let mut policy = ReplicationPolicy::new(ReplicationTarget::MemoryNode("node".into()));
    policy
        .pipeline
        .add_operator(RxOp::Throttle(Duration::from_secs(10)));
    let mut p = MemoryPublication::new(&policy);
    for (secs, n) in [(0, 1), (1, 2), (2, 3)] {
        p.update(
            &lua,
            Some(Arc::new(serde_json::json!(n))),
            t + Duration::from_secs(secs),
        )
        .unwrap();
    }
    assert_eq!(p.published.as_deref(), Some("1"));
    p.update(&lua, p.observed.clone(), t + Duration::from_secs(10))
        .unwrap();
    assert_eq!(p.published.as_deref(), Some("3"));
}

#[test]
fn memory_prefix_delete_withdraws_leaf_and_frozen_snapshot() {
    let f = Fixture::new();
    f.policy(
        ReplicationTarget::MemoryNode("profile/name".into()),
        vec![RxOp::Take(1)],
    );
    f.source("profile/name", serde_json::json!("Alice"));
    f.snapshots();
    assert!(
        f.replicator
            .affected_memory_nodes("profile/")
            .contains(&"profile/name".into())
    );
    f.replicator.mark_update(ReplicationMark::MemoryDelete {
        key: "profile/".into(),
    });
    assert_eq!(
        f.snapshots()[&f.player].rooms["world"].state_remove,
        ["profile/name"]
    );
    f.source("profile/name", serde_json::json!("Bob"));
    assert!(f.snapshots().is_empty()); // completed take does not restart after deletion
}

#[test]
fn memory_delete_then_recreate_in_same_tick_invalidates_frozen_copy() {
    let f = Fixture::new();
    f.policy(
        ReplicationTarget::MemoryNode("node".into()),
        vec![RxOp::Take(1)],
    );
    f.source("node", serde_json::json!(1));
    f.snapshots();
    f.replicator
        .mark_update(ReplicationMark::MemoryDelete { key: "node".into() });
    f.source("node", serde_json::json!(2));
    assert_eq!(
        f.snapshots()[&f.player].rooms["world"].state_remove,
        ["node"]
    );
}

#[test]
fn memory_operator_can_revoke_its_policy_without_borrow_panic_or_resurrection() {
    let f = Fixture::new();
    let replicator = f.replicator.clone();
    let predicate = f
        .lua
        .create_function(move |lua, _: i64| {
            // Both mutable world access and replicator access must be available.
            get_app_data_mut::<app_data::World>(lua)?
                .0
                .spawn((Room(1),));
            replicator.revoke_policies_by_target(&ReplicationTarget::MemoryNode("node".into()));
            Ok(true)
        })
        .unwrap();
    let id = f.policy(
        ReplicationTarget::MemoryNode("node".into()),
        vec![RxOp::Filter(predicate)],
    );
    f.source("node", serde_json::json!(1));
    assert!(f.snapshots().is_empty());
    assert!(
        !f.replicator
            .inner
            .borrow()
            .memory_publications
            .contains_key(&id)
    );
}

#[test]
fn memory_pipeline_runs_once_per_revision_and_is_shared_by_viewers() {
    let f = Fixture::new();
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = calls.clone();
    let map = f
        .lua
        .create_function(move |_, n: i64| {
            count.set(count.get() + 1);
            Ok(n * 10)
        })
        .unwrap();
    f.policy(
        ReplicationTarget::MemoryNode("node".into()),
        vec![RxOp::Map(map)],
    );
    f.source("node", serde_json::json!(1));
    f.snapshots();
    f.source("node", serde_json::json!(1));
    f.snapshots();
    assert_eq!(calls.get(), 1);
    let newcomer = PlayerKey {
        slot_idx: 1,
        generation: 0,
    };
    let anchor = f.anchor();
    f.replicator
        .inner
        .borrow_mut()
        .room_to_anchors
        .get_mut(&1)
        .unwrap()
        .insert(PlayerAnchor {
            pk: newcomer,
            entity: anchor,
        });
    assert_eq!(f.snapshots()[&newcomer].rooms["world"].state["node"], "10");
    assert_eq!(calls.get(), 1);
    f.source("node", serde_json::json!(2));
    let snapshots = f.snapshots();
    assert_eq!(snapshots[&f.player].rooms["world"].state["node"], "20");
    assert_eq!(snapshots[&newcomer].rooms["world"].state["node"], "20");
    assert_eq!(calls.get(), 2);
}
