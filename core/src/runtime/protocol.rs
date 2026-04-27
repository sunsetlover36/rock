use shared::{IncomingRequest, PlayerKey, SocketConnectionQuery, farcaster::WebhookEvent};

use crate::{envelope::ClientEnvelope, socket::protocol::ServerMessage};

pub type ClientRequest = ClientEnvelope<IncomingRequest>;

pub trait GameModeClientApi: Send + Sync {
    fn has(&self, pk: PlayerKey) -> bool;
    fn list(&self) -> Vec<PlayerKey>;
    fn send(&self, message: ServerMessage);
}

pub enum SystemCallback {
    PlayerConnect {
        pk: PlayerKey,
        connection_params: SocketConnectionQuery,
        identity: Option<String>,
    },
    PlayerDisconnect {
        pk: PlayerKey,
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
