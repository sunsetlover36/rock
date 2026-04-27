use std::sync::Arc;

use shared::PlayerKey;
use tokio::sync::mpsc;

use super::protocol::{PlayerConnection, Session, SessionCommand, SessionRegistryState};

#[derive(Clone)]
pub struct SessionRegistrar {
    pub(super) inner: Arc<SessionRegistryState>,
}
impl SessionRegistrar {
    pub fn register(&self, identity: Option<String>) -> PlayerConnection {
        let pk = {
            let mut pool = self.inner.player_pool.lock();
            pool.claim()
        };
        let (tx, rx) = mpsc::channel::<SessionCommand>(self.inner.session_channel_buffer);

        self.inner.sessions.insert(pk, Session { identity, tx });
        PlayerConnection::new(pk, rx, self.inner.broadcast_hub.subscribe(), self.clone())
    }
    pub fn unregister(&self, pk: &PlayerKey) {
        self.inner.sessions.remove(pk);
        self.inner.player_pool.lock().release(pk);
    }
}
