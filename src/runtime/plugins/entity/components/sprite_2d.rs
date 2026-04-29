use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Sprite2D(pub rock_wire::components::Sprite2D);
