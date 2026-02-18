use serde::{Deserialize, Serialize};

use crate::runtime::api::plugins::entity::components::Vector2D;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Sprite2D {
    texture: String,
    scale: Vector2D,
    layer: u32,
    visible: bool,
}
