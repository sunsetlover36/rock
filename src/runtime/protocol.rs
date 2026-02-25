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
pub trait GameModeClientApi: Send {
    fn send(&self, event: GameModeClientCommand);
}

pub enum SystemCallback {
    OnPlayerConnect { pk: PlayerKey },
    OnPlayerDisconnect { pk: PlayerKey },
}
pub enum RuntimeCallback {
    System(SystemCallback),
    Client(ClientRequest),
}
