use rock_wire::{
    IncomingRequest, PlayerKey, SocketConnectionQuery,
    farcaster::{Fid, WebhookEvent},
};

use crate::{envelope::ClientEnvelope, socket::protocol::ServerMessage};

pub type ClientRequest = ClientEnvelope<IncomingRequest>;

pub trait GameModeClientApi: Send + Sync {
    fn has(&self, pk: PlayerKey) -> bool;
    fn list(&self) -> Vec<PlayerKey>;
    fn send(&self, message: ServerMessage);
    fn identity(&self, pk: PlayerKey) -> Option<String>;
    fn fid(&self, pk: PlayerKey) -> Option<Fid>;
}

pub enum SystemCallback {
    PlayerConnect {
        pk: PlayerKey,
        connection_params: SocketConnectionQuery,
    },
    PlayerDisconnect {
        identity: Option<String>,
    },
    ImpromptuRequest {
        name: Option<String>,
        code: String,
    },
    Webhook(Box<WebhookEvent>),
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
