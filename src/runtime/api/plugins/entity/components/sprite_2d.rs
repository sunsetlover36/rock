use serde::Deserialize;

use crate::runtime::api::plugins::entity::components::Vector2D;

#[derive(Clone, Debug, Deserialize])]
pub(crate) struct Sprite2D {
    texture: String,
    scale: Vector2D,
    layer: u32,
    visible: bool,
}
