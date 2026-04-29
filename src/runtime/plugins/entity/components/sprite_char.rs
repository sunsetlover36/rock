use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SpriteChar(pub rock_wire::components::SpriteChar);
