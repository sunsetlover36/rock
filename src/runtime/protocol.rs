use shared::{IncomingRequest, PlayerKey};

use crate::{envelope::ClientEnvelope, socket::protocol::ServerMessage};

pub type ClientRequest = ClientEnvelope<IncomingRequest>;

pub trait GameModeClientApi: Send + Sync {
    fn has(&self, pk: PlayerKey) -> bool;
    fn list(&self) -> Vec<PlayerKey>;
    fn send(&self, message: ServerMessage);
}

pub enum SystemCallback {
    PlayerConnect { pk: PlayerKey },
    PlayerDisconnect { pk: PlayerKey },
    ImpromptuRequest { name: Option<String>, code: String },
}

pub enum RuntimeCallback {
    System(SystemCallback),
    Client(ClientRequest),
}

pub enum RuntimeCommand {
    Reload,
    Shutdown,
}
pub enum RuntimeExit {
    Reload,
    Shutdown,
}
