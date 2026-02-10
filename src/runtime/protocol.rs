use shared::{GameModeClientRequest, PlayerKey};

use crate::{actor::indexer::protocol::IndexerEvent, envelope::ClientEnvelope};

pub type ClientRequest = ClientEnvelope<GameModeClientRequest>;

#[derive(Debug, Clone)]
pub enum GameModeClientCommand {
    SendMessage { pk: PlayerKey, text: String },
    Broadcast { text: String },
    Log { text: String },
    KickPlayer { pk: PlayerKey },
}
pub trait GameModeClientApi: Send {
    fn send(&self, event: GameModeClientCommand);
}

pub enum SystemCallback {
    OnPlayerConnect { pk: PlayerKey },
}
pub enum RuntimeCallback {
    System(SystemCallback),
    Client(ClientRequest),
    Indexer(IndexerEvent),
}

// Lua event listeners (gamemode events)
#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub enum WorldEvent {
    Awake,
}

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub enum PlayerEvent {
    Connect,
}

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub enum GameModeEvent {
    World(WorldEvent),
    Player(PlayerEvent),
}
