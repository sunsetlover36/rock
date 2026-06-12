// session_registry.rs
// A unified WebSocket session and delivery layer

use dashmap::DashMap;
use rock_wire::OutgoingPacket;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::{
    player_pool::PlayerPool,
    socket::session_registry::protocol::{SessionBackpressureStats, SessionRegistryState},
};

pub mod protocol;
pub mod registrar;
pub use registrar::SessionRegistrar;
pub mod sender;
pub use sender::SessionSender;

pub struct SessionRegistryParams {
    pub broadcast_hub_buffer: usize,
    pub session_channel_buffer: usize,
    pub player_pool: PlayerPool,
    pub tokio_handle: tokio::runtime::Handle,
}

pub struct SessionRegistry {
    inner: Arc<SessionRegistryState>,
    tokio_handle: tokio::runtime::Handle,
}
impl SessionRegistry {
    pub fn new(params: SessionRegistryParams) -> Self {
        let sessions = DashMap::new();
        let (broadcast_hub, _) = broadcast::channel::<OutgoingPacket>(params.broadcast_hub_buffer);

        Self {
            inner: Arc::new(SessionRegistryState {
                player_pool: parking_lot::Mutex::new(params.player_pool),
                broadcast_hub,
                session_channel_buffer: params.session_channel_buffer,
                sessions,
                stats: Arc::new(SessionBackpressureStats::default()),
            }),
            tokio_handle: params.tokio_handle,
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
            tokio_handle: self.tokio_handle.clone(),
        }
    }
}
