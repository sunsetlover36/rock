use serde::{Deserialize, Serialize};
use shared::components::Vector2D;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Sprite2D {
    texture: String,
    scale: Vector2D,
    layer: u32,
    visible: bool,
}
