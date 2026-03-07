use serde::{Deserialize, Serialize};
use shared::PlayerId;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct OwnedBy(pub PlayerId);
