use serde::{Deserialize, Serialize};

use crate::runtime::api::plugins::entity::components::Vector2D;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Transform2D {
    translation: Vector2D,
    rotation: u8,
}
