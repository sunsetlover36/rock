use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct Position(pub rock_wire::components::Position);
