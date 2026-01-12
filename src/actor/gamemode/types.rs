use shared::{IndexerEvent, PlayerKey};

use crate::actor::ws::client_message::ClientMessage;

#[async_trait::async_trait]
pub trait GameModeEventListener: Send + Sync {
    async fn on_emit(&self, event: GameModeEvent);
}

pub enum GameModeEvent {
    SendClientMessage { pk: PlayerKey, text: String },
    Broadcast { text: String },
    Log { text: String },
}

pub enum GameModeCallback {
    Client(ClientMessage),
    Indexer(IndexerEvent),
}
