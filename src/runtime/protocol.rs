use shared::{IncomingRequest, PlayerKey};

use crate::envelope::ClientEnvelope;

pub type ClientRequest = ClientEnvelope<IncomingRequest>;

#[derive(Debug, Clone)]
pub enum GameModeClientCommand {
    SendMessage { pk: PlayerKey, text: String },
    Broadcast { text: String },
    Log { text: String },
    KickPlayer { pk: PlayerKey },
}
pub trait GameModeClientApi: Send + Sync {
    fn has(&self, pk: PlayerKey) -> bool;
    fn list(&self) -> Vec<PlayerKey>;
    fn send(&self, event: GameModeClientCommand);
}

pub enum SystemCallback {
    OnPlayerConnect { pk: PlayerKey },
    OnPlayerDisconnect { pk: PlayerKey },
    OnImpromptuRequest { name: Option<String>, code: String },
}
pub enum RuntimeCallback {
    System(SystemCallback),
    Client(ClientRequest),
}
