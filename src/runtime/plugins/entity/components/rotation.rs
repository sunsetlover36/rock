use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct Rotation(pub rock_wire::components::Rotation);
