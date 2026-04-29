use serde::{Deserialize, Serialize};
use rock_wire::PlayerId;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct OwnedBy(pub PlayerId);
