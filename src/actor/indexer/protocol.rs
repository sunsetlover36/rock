use serde::{self, Deserialize, Serialize};
use shared::Tile;

pub enum RedisChannel {
    Tile,
}
impl RedisChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RedisChannel::Tile => "tile",
        }
    }
}

pub struct RedisMessage {
    pub channel: RedisChannel,
    pub payload: IndexerEvent,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IndexerEvent {
    TileExplored { tile: Tile },
    TileRemoved { tx_hash: String, x: i64, y: i64 },
}
