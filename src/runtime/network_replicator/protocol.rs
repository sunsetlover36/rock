use std::time::Duration;

use shared::Position;

use crate::runtime::plugins::entity::{
    BlueprintId,
    components::{ComponentKey, CustomDataComponent},
};

#[derive(Debug, Eq, PartialEq, Hash)]
pub(crate) enum EntityDirtyComponent {
    Core(ComponentKey),
    Custom(CustomDataComponent),
}

pub(crate) enum ReplicationMark {
    Entity {
        id: hecs::Entity,
        component: EntityDirtyComponent,
    },
    Memory(String),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) enum ReplicationTarget {
    Blueprint(BlueprintId),
    Entity(hecs::Entity),
    MemoryNode(String),
}

#[derive(Debug, Clone)]
pub(crate) struct ReplicationPolicy {
    pub only_fields: Vec<String>,
    pub hidden_fields: Vec<String>,
    pub room: Option<String>,
    pub radius: Option<u32>,
    pub nearest: Option<Position>,
    pub throttle: Option<Duration>,
}
