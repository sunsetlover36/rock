use serde::{Deserialize, Serialize};

use crate::runtime::network_replicator::protocol::RoomId;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct Room(pub RoomId);
