use serde::{Deserialize, Serialize};
use shared::PlayerId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OwnedBy(pub PlayerId);
