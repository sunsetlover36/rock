use super::components::ComponentKey;
use crate::runtime::plugins::on::protocol::{EntityEventKey, EventDescriptor, GameModeEventKey};

pub(crate) const ENTITY_EVENT_DESCRIPTORS: &[EventDescriptor] = &[
    EventDescriptor {
        namespace: None,
        name: "move",
        event_key: GameModeEventKey::Entity(EntityEventKey::ComponentUpdate(
            ComponentKey::Position,
        )),
    },
    EventDescriptor {
        namespace: None,
        name: "custom",
        event_key: GameModeEventKey::Entity(EntityEventKey::CustomDataUpdate),
    },
];
