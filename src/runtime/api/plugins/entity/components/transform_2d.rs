use serde::Deserialize;

use crate::runtime::api::plugins::entity::components::Vector2D;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Transform2D {
    translation: Vector2D,
    rotation: u8,
}
