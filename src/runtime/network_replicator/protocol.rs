use mlua::{FromLua, LuaSerdeExt};
use rock_wire::components::Position;
use serde::Deserialize;
use slotmap::new_key_type;
use strum::{AsRefStr, EnumIter};

use crate::{
    runtime::plugins::entity::{BlueprintId, components::ComponentData},
    rx::RxPipeline,
};

new_key_type! {
    pub(crate) struct PolicyId;
}

#[derive(Clone, Debug, Copy, Eq, PartialEq, Hash, Deserialize, EnumIter, AsRefStr)]
pub(crate) enum AreaShape {
    Circle,
    Square,
    Diamond,
}
impl FromLua for AreaShape {
    fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
        lua.from_value(value)
    }
}

#[derive(Clone, Debug, Copy, Deserialize)]
pub struct Area {
    pub position: Position,
    pub radius: f32,
    pub shape: AreaShape,
}
impl Area {
    pub fn contains(&self, position: Position) -> bool {
        let dx = (self.position.x - position.x).abs();
        let dy = (self.position.y - position.y).abs();

        match self.shape {
            AreaShape::Circle => dx * dx + dy * dy <= self.radius * self.radius,
            AreaShape::Square => dx.max(dy) <= self.radius,
            AreaShape::Diamond => dx + dy <= self.radius,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RadialArea {
    pub radius: f32,
    pub shape: AreaShape,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SpatialFilter {
    Global,
    Radius(RadialArea),
    Area(Area),
}

pub(crate) type RoomId = u64;

#[derive(Debug, Clone)]
pub(crate) enum EntityDirtyComponent {
    Core(ComponentData),
    CustomField(String),
}

pub(crate) enum EntityReplicationAction {
    Spawn(RoomId),
    Update(EntityDirtyComponent),
    Warp {
        from: Option<RoomId>,
        to: Option<RoomId>,
    },
    Despawn(RoomId),
}
pub(crate) enum ReplicationMark {
    Entity {
        entity: hecs::Entity,
        action: EntityReplicationAction,
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
