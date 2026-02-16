use crate::runtime::api::on::{EventDescriptor, GameModeEventKey, PlayerEventKey, WorldEventKey};

pub(crate) const GLOBAL_EVENT_DESCRIPTORS: &[EventDescriptor] = &[
    EventDescriptor {
        namespace: Some("world"),
        name: "awake",
        event_key: GameModeEventKey::World(WorldEventKey::Awake),
    },
    EventDescriptor {
        namespace: Some("player"),
        name: "connect",
        event_key: GameModeEventKey::Player(PlayerEventKey::Connect),
    },
];
