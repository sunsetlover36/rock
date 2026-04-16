use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use strum::EnumDiscriminants;

pub mod components;

#[derive(Serialize_repr, Deserialize_repr, Debug, Clone)]
#[repr(u8)]
pub enum TileKind {
    BASE = 0,
    SNOW = 1,
}

#[derive(Serialize_repr, Deserialize_repr, Debug, Clone)]
#[repr(u8)]
pub enum ObjectId {
    NONE = 0,
    TREE = 1,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Tile {
    pub x: i64,
    pub y: i64,
    pub owner: String,
    pub tx_hash: String,
    pub block_number: i64,
    pub kind: i16,
    #[serde(rename = "objectId")]
    pub object_id: i16,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// -- State -> state.rs
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Position {
    x: i64,
    y: i64,
}

// -- Player -> ?
#[derive(Serialize, Deserialize, Debug)]
pub struct PlayerData {
    pub position: Position,
}
// --

// -- Player pool -> player_pool.rs
pub type PlayerId = u64;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct PlayerKey {
    pub slot_idx: u32,
    pub generation: u32,
}
impl PlayerKey {
    pub fn pack(&self) -> PlayerId {
        ((self.generation as u64) << 32) | self.slot_idx as u64
    }
    pub fn unpack(id: PlayerId) -> Self {
        let slot_idx = (id & 0xFFFF_FFFF) as u32;
        let generation = (id >> 32) as u32;
        Self {
            slot_idx,
            generation,
        }
    }
}
// --

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct EntityData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<components::Name>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<components::Speed>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub owned_by: Option<components::OwnedBy>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprite: Option<components::Sprite2D>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub char: Option<components::SpriteChar>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<components::Position>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<components::Rotation>,

    pub custom: serde_json::Map<String, serde_json::Value>,
}

// -- Communication -> session_registry.rs, [actor] server_message.rs, [commit router] ws_router.rs, client_protocol.rs
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SignalPacket {
    pub name: Option<String>,
    pub data: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RoomSnapshot {
    // Entities
    pub spawn: HashMap<u32, EntityData>,
    pub update: HashMap<u32, EntityData>,

    // Arbitrary storage (e.g., database)
    pub state: HashMap<String, String>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorldSnapshot {
    pub tick: u64,
    pub rooms: HashMap<u64, RoomSnapshot>,

    // Entities
    pub despawn: Vec<u32>,
}
impl WorldSnapshot {
    pub fn new(tick: u64) -> Self {
        Self {
            tick,
            rooms: HashMap::new(),
            despawn: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SystemPacket {
    PlayerKicked,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "t", content = "d", rename_all = "lowercase")]
pub enum OutgoingPacket {
    World(WorldSnapshot),
    Signal(SignalPacket),
    System(SystemPacket),
}

#[derive(Deserialize, Serialize, Debug, Clone, EnumDiscriminants)]
#[strum_discriminants(name(InputKind))]
#[serde(untagged)]
pub enum InputData {
    Vector2D { x: f32, y: f32 },
    Button(bool),
    Axis(f32),
}
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct InputAction {
    pub id: u16,
    pub data: InputData,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "t", content = "d", rename_all = "lowercase")]
pub enum IncomingRequest {
    Input(InputAction),
    Chat(String),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ImpromptuRequest {
    pub name: Option<String>,
    pub code: String,
}
// --
