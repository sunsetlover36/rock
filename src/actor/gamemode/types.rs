use shared::{ClientMessage, IndexerEvent, PlayerKey};

pub trait GameModeEventListener: Send + Sync {
    fn on_emit(&self, event: GameModeEvent);
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
