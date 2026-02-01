// session_registry.rs
// A unified WebSocket session and delivery layer

use dashmap::DashMap;
use shared::OutgoingPacket;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::{player_pool::PlayerPool, socket::session_registry::protocol::SessionRegistryState};

pub mod protocol;
pub mod registrar;
pub use registrar::{Session, SessionRegistrar};
pub mod sender;
pub use sender::SessionSender;

pub struct SessionRegistry {
    inner: Arc<SessionRegistryState>,
}
impl SessionRegistry {
    pub fn new(
        broadcast_hub_buffer: usize,
        session_channel_buffer: usize,
        player_pool: PlayerPool,
    ) -> Self {
        let sessions = DashMap::new();
        let (broadcast_hub, _) = broadcast::channel::<OutgoingPacket>(broadcast_hub_buffer);

        Self {
            inner: Arc::new(SessionRegistryState {
                player_pool: parking_lot::Mutex::new(player_pool),
                broadcast_hub,
                session_channel_buffer,
                sessions,
            }),
        }
    }

    pub fn registrar(&self) -> SessionRegistrar {
        SessionRegistrar {
            inner: self.inner.clone(),
        }
    }
    pub fn sender(&self) -> SessionSender {
        SessionSender {
            inner: self.inner.clone(),
        }
    }
}
