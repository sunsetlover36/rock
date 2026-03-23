use shared::{IncomingRequest, PlayerKey};

use crate::{envelope::ClientEnvelope, socket::protocol::ServerMessage};

pub type ClientRequest = ClientEnvelope<IncomingRequest>;

pub trait GameModeClientApi: Send + Sync {
    fn has(&self, pk: PlayerKey) -> bool;
    fn list(&self) -> Vec<PlayerKey>;
    fn send(&self, message: ServerMessage);
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
