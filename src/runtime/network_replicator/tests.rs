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
    replicator: NetworkReplicator,
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
        let replicator = NetworkReplicator::new(Arc::new(NoClients));
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

    fn policy(&self, target: ReplicationTarget, operators: Vec<RxOp>) {
        let mut policy = ReplicationPolicy::new(target);
        if matches!(policy.target, ReplicationTarget::MemoryNode(_)) {
            policy.routing = PolicyRouting::Pinned(1);
        }
        let mut pipeline = RxPipeline::default();
        for op in operators {
            pipeline.add_operator(op);
        }
        policy.pipeline = pipeline;
        self.replicator.commit_policy(policy).unwrap();
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
    f.replicator
        .inner
        .borrow_mut()
        .memory_snapshots
        .insert(key.clone(), serde_json::json!(0));
    assert!(f.snapshots().is_empty());
    f.replicator
        .inner
        .borrow_mut()
        .memory_snapshots
        .insert(key.clone(), serde_json::json!(2));
    let snapshots = f.snapshots();
    assert_eq!(snapshots[&f.player].rooms["world"].state[&key], "20");
    assert!(f.snapshots().is_empty());
}
