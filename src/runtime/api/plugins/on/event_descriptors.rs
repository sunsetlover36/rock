use crate::runtime::{GameModeEvent, PlayerEvent, WorldEvent};

pub(super) struct EventDescriptor {
    pub namespace: &'static str,
    pub name: &'static str,
    pub event: GameModeEvent,
}

pub(super) const EVENT_DESCRIPTORS: &[EventDescriptor] = &[
    EventDescriptor {
        namespace: "world",
        name: "awake",
        event: GameModeEvent::World(WorldEvent::Awake),
    },
    EventDescriptor {
        namespace: "player",
        name: "connect",
        event: GameModeEvent::Player(PlayerEvent::Connect),
    },
];
