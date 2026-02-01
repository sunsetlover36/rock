use std::sync::Arc;

use shared::{OutgoingPacket, PlayerKey};
use tokio::sync::{broadcast, mpsc};

use crate::socket::session_registry::protocol::{SessionCommand, SessionRegistryState};

pub struct Session {
    pub pk: PlayerKey,
    pub session_rx: mpsc::Receiver<SessionCommand>,
    pub broadcast_rx: broadcast::Receiver<OutgoingPacket>,
    registrar: SessionRegistrar,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.registrar.unregister(&self.pk);
    }
}

#[derive(Clone)]
pub struct SessionRegistrar {
    pub(super) inner: Arc<SessionRegistryState>,
}
impl SessionRegistrar {
    pub fn register(&self) -> Session {
        let pk = {
            let mut pool = self.inner.player_pool.lock();
            pool.claim()
        };
        let (tx, rx) = mpsc::channel::<SessionCommand>(self.inner.unicast_channel_buffer);

        self.inner.sessions.insert(pk, tx);
        Session {
            pk,
            session_rx: rx,
            broadcast_rx: self.inner.broadcast_hub.subscribe(),
            registrar: self.clone(),
        }
    }
    pub fn unregister(&self, pk: &PlayerKey) {
        self.inner.sessions.remove(pk);
        self.inner.player_pool.lock().release(pk);
    }
}
