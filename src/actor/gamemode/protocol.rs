use shared::{GameModeRequest, PlayerKey};

use crate::{actor::indexer::protocol::IndexerEvent, client_protocol::Envelope};

pub type ClientRequest = Envelope<GameModeRequest>;

pub enum GameModeEvent {
    SendClientMessage { pk: PlayerKey, text: String },
    Broadcast { text: String },
    Log { text: String },
}

pub enum EngineCallback {
    OnGameModeInit,
    OnPlayerConnect { pk: PlayerKey },
}

pub enum GameModeCallback {
    Engine(EngineCallback),
    Client(ClientRequest),
    Indexer(IndexerEvent),
}
