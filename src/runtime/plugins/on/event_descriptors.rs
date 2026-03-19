use super::protocol::{
    EventDescriptor, GameModeEventKey, PlayerEventKey, TimerEventKey, WorldEventKey,
};

pub(crate) const GLOBAL_EVENT_DESCRIPTORS: &[EventDescriptor] = &[
    EventDescriptor {
        namespace: Some("world"),
        name: "awake",
        event_key: GameModeEventKey::World(WorldEventKey::Awake),
    },
    EventDescriptor {
        namespace: Some("world"),
        name: "impromptu",
        event_key: GameModeEventKey::World(WorldEventKey::Impromptu),
    },
    EventDescriptor {
        namespace: Some("player"),
        name: "online",
        event_key: GameModeEventKey::Player(PlayerEventKey::Online),
    },
    EventDescriptor {
        namespace: Some("player"),
        name: "offline",
        event_key: GameModeEventKey::Player(PlayerEventKey::Offline),
    },
    EventDescriptor {
        namespace: Some("player"),
        name: "input",
        event_key: GameModeEventKey::Player(PlayerEventKey::Input),
    },
    EventDescriptor {
        namespace: Some("player"),
        name: "enter",
        event_key: GameModeEventKey::Player(PlayerEventKey::Enter),
    },
    EventDescriptor {
        namespace: Some("player"),
        name: "exit",
        event_key: GameModeEventKey::Player(PlayerEventKey::Exit),
    },
    EventDescriptor {
        namespace: Some("timer"),
        name: "fire",
        event_key: GameModeEventKey::Timer(TimerEventKey::Fire),
    },
];
