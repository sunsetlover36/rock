use shared::components::RadialArea;
use slotmap::new_key_type;

use crate::{
    runtime::plugins::entity::{BlueprintId, components::ComponentData},
    rx::RxPipeline,
};

new_key_type! {
    pub(crate) struct PolicyId;
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SpatialFilter {
    Global,
    Radius(f32),
    Area(RadialArea),
}

pub(crate) type RoomId = u64;

#[derive(Debug, Clone)]
pub(crate) enum EntityDirtyComponent {
    Core(ComponentData),
    Custom,
}

pub(crate) enum ReplicationMark {
    Entity {
        id: hecs::Entity,
        component: EntityDirtyComponent,
    },
    Memory {
        key: String,
        value: serde_json::Value,
    },
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) enum ReplicationTarget {
    Blueprint(BlueprintId),
    Entity(hecs::Entity),
    MemoryNode(String),
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
    pub fields_mask: u64,
    pub spatial: SpatialFilter,
    pub pipeline: RxPipeline,
}
impl ReplicationPolicy {
    pub fn new(target: ReplicationTarget) -> Self {
        Self {
            target,
            routing: PolicyRouting::DynamicFollow,
            fields_mask: u64::MAX,
            spatial: SpatialFilter::Global,
            pipeline: RxPipeline::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum PolicyFieldUpdate {
    Spatial { filter: SpatialFilter },
    Room { id: RoomId },
}
