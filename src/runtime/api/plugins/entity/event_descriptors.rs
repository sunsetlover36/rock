use crate::runtime::api::on::{EntityEventKey, EventDescriptor, GameModeEventKey};

pub(crate) const ENTITY_EVENT_DESCRIPTORS: &[EventDescriptor] = &[
    EventDescriptor {
        namespace: None,
        name: "move",
        event_key: GameModeEventKey::Entity(EntityEventKey::ComponentUpdate(
            super::ComponentKey::Position,
        )),
    },
    EventDescriptor {
        namespace: None,
        name: "custom",
        event_key: GameModeEventKey::Entity(EntityEventKey::CustomDataUpdate),
    },
];
