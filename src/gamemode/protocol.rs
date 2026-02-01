use shared::{GameModeRequest, PlayerKey};

use crate::{actor::indexer::protocol::IndexerEvent, envelope::ClientEnvelope};

pub type ClientRequest = ClientEnvelope<GameModeRequest>;

pub enum GameModeEvent {
    SendClientMessage { pk: PlayerKey, text: String },
    Broadcast { text: String },
    Log { text: String },
    KickPlayer { pk: PlayerKey },
}
pub trait GameModeEventListener: Send {
    fn emit(&self, event: GameModeEvent);
}

pub enum EngineCallback {
    OnPlayerConnect { pk: PlayerKey },
}

pub enum GameModeCallback {
    Engine(EngineCallback),
    Client(ClientRequest),
    Indexer(IndexerEvent),
}
