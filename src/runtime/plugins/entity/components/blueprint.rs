use serde::{Deserialize, Serialize};

use crate::runtime::plugins::entity::blueprint::BlueprintId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Blueprint(pub BlueprintId);
