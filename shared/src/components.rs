use serde::{Deserialize, Serialize};

use crate::PlayerId;

pub type Name = String;
pub type Speed = u32;
pub type OwnedBy = PlayerId;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sprite2D {
    pub texture: String,
    pub scale: Vector2D,
    pub layer: u32,
    pub visible: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpriteChar {
    pub char: String,
    pub color: String,
    pub bg_color: Option<String>,
    pub visible: bool,
}

#[derive(Clone, Debug, Copy, Serialize, Deserialize)]
pub struct Vector2D {
    pub x: f32,
    pub y: f32,
}
impl Vector2D {
    pub fn distance_squared(&self, other: &Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx * dx + dy * dy
    }

    pub fn distance(&self, other: &Self) -> f32 {
        self.distance_squared(other).sqrt()
    }
}
pub type Position = Vector2D;

pub type Rotation = u8;

#[derive(Clone, Debug, Copy, Serialize, Deserialize)]
pub struct RadialArea {
    pub position: Position,
    pub radius: f32,
}
