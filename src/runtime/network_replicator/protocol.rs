use std::time::Duration;

use shared::{PlayerKey, components::RadialArea};

use crate::runtime::plugins::entity::{
    BlueprintId,
    components::{ComponentKey, CustomDataComponent},
};

#[derive(Debug, Clone)]
pub(crate) enum SpatialFilter {
    Global,
    Radius(u32),
    Area(RadialArea),
}

pub(crate) type RoomId = u64;

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
    Player(PlayerKey),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum PolicyRouting {
    DynamicFollow,
    Pinned(RoomId),
}

#[derive(Debug, Clone)]
pub(crate) struct ReplicationPolicy {
    pub target: ReplicationTarget,
    pub routing: PolicyRouting,
    pub only_fields: Vec<String>,
    pub hidden_fields: Vec<String>,
    pub room: Option<RoomId>,
    pub spatial: Option<SpatialFilter>,
    pub throttle: Option<Duration>,
}
impl ReplicationPolicy {
    pub fn new(target: ReplicationTarget) -> Self {
        Self {
            target,
            // Policy automatically follows any room given to its target
            routing: PolicyRouting::DynamicFollow,
            only_fields: Vec::new(),
            hidden_fields: Vec::new(),
            room: None,
            spatial: None,
            throttle: None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum PolicyFieldUpdate {
    Spatial { filter: Option<SpatialFilter> },
    Room { id: Option<RoomId> },
    Throttle { throttle: Option<Duration> },
}

#[derive(Debug, Clone)]
pub(crate) struct PendingSignal {
    pub name: Option<String>,
    pub data: serde_json::Map<String, serde_json::Value>,
    pub area: Option<RadialArea>,
    pub scope: SignalScope,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SignalScope {
    Global,
    Player(PlayerKey),
}
