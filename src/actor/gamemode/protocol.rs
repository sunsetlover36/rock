use shared::{ClientMessage, PlayerKey};

use crate::actor::indexer::protocol::IndexerEvent;

pub enum GameModeEvent {
    SendClientMessage { pk: PlayerKey, text: String },
    Broadcast { text: String },
    Log { text: String },
}

pub enum GameModeCallback {
    Client(ClientMessage),
    Indexer(IndexerEvent),
}
