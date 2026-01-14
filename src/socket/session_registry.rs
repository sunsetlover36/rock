// session_registry.rs
// A unified WebSocket session and delivery layer

use dashmap::DashMap;
use shared::{OutgoingPacket, PlayerKey};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

use crate::{
    player_pool::PlayerPool,
    socket::protocol::{Recipient, ServerMessage},
};

#[derive(Debug)]
pub enum SessionSendError {
    NoSuchSession,
    Prohibited,
    ChannelFull(OutgoingPacket),
    ChannelClosed(OutgoingPacket),
}
impl From<mpsc::error::TrySendError<OutgoingPacket>> for SessionSendError {
    fn from(err: mpsc::error::TrySendError<OutgoingPacket>) -> Self {
        match err {
            mpsc::error::TrySendError::Full(p) => SessionSendError::ChannelFull(p),
            mpsc::error::TrySendError::Closed(p) => SessionSendError::ChannelClosed(p),
        }
    }
}
impl From<mpsc::error::SendError<OutgoingPacket>> for SessionSendError {
    fn from(err: mpsc::error::SendError<OutgoingPacket>) -> Self {
        SessionSendError::ChannelClosed(err.0)
    }
}

struct RegistryInner {
    player_pool: parking_lot::Mutex<PlayerPool>,
    broadcast_hub: broadcast::Sender<OutgoingPacket>,
    unicast_channel_buffer: usize,
    sessions: DashMap<PlayerKey, mpsc::Sender<OutgoingPacket>>,
}
pub struct SessionRegistry {
    inner: Arc<RegistryInner>,
}
impl SessionRegistry {
    pub fn new(
        broadcast_hub_buffer: usize,
        unicast_channel_buffer: usize,
        player_pool: PlayerPool,
    ) -> Self {
        let sessions = DashMap::new();
        let (broadcast_hub, _) = broadcast::channel::<OutgoingPacket>(broadcast_hub_buffer);

        Self {
            inner: Arc::new(RegistryInner {
                player_pool: parking_lot::Mutex::new(player_pool),
                broadcast_hub,
                unicast_channel_buffer,
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

pub struct Session {
    pub pk: PlayerKey,
    pub unicast_rx: mpsc::Receiver<OutgoingPacket>,
    pub broadcast_rx: broadcast::Receiver<OutgoingPacket>,
}

#[derive(Clone)]
pub struct SessionRegistrar {
    inner: Arc<RegistryInner>,
}
impl SessionRegistrar {
    pub fn register(&self) -> Session {
        let pk = {
            let mut pool = self.inner.player_pool.lock();
            pool.claim()
        };
        let (tx, rx) = mpsc::channel::<OutgoingPacket>(self.inner.unicast_channel_buffer);

        self.inner.sessions.insert(pk, tx);
        Session {
            pk,
            unicast_rx: rx,
            broadcast_rx: self.inner.broadcast_hub.subscribe(),
        }
    }
    pub fn unregister(&self, pk: &PlayerKey) {
        self.inner.sessions.remove(pk);
        self.inner.player_pool.lock().release(pk);
    }
}

#[derive(Clone)]
pub struct SessionSender {
    inner: Arc<RegistryInner>,
}
impl SessionSender {
    fn get_channel(&self, pk: &PlayerKey) -> Option<mpsc::Sender<OutgoingPacket>> {
        self.inner.sessions.get(pk).map(|tx| tx.value().clone())
    }

    // Cases:
    // -> 1. message.delivery == Reliable and message.recipient == All -> Prohibited (can't guarantee it right now)
    // -> 2. message.delivery == Ephemeral and message.recipient == All -> Sync [broadcast]
    // -> 3. message.delivery == Reliable and message.recipient != All -> Async [unicast: send().await]
    // -> 4. message.delivery == Ephemeral and message.recipient != All -> Sync [unicast: try_send()]
    // -> 5. message.delivery == Reliable and message.recipient == Except -> Prohibited (can't guarantee it right now)
    //
    // TODO: Add a timeout for slow consumers with a forced disconnection opportunity
    // -> To prevent blocking a thread
    // -> Required for a reliable message delivery to multiple recipients
    pub fn send_ephemeral(&self, message: ServerMessage) -> Result<(), SessionSendError> {
        match message.recipient {
            Recipient::All => {
                let _ = self.inner.broadcast_hub.send(message.packet);
            }
            Recipient::Single(pk) => {
                let tx = self
                    .get_channel(&pk)
                    .ok_or(SessionSendError::NoSuchSession)?;
                let _ = tx.try_send(message.packet);
            }
            Recipient::List(pks) => {
                for pk in pks {
                    if let Some(tx) = self.get_channel(&pk) {
                        let _ = tx.try_send(message.packet.clone());
                    }
                }
            }
            Recipient::Except(except_pk) => {
                for entry in self.inner.sessions.iter() {
                    let pk = *entry.key();
                    if pk == except_pk {
                        continue;
                    }

                    let tx = entry.value().clone();
                    let _ = tx.try_send(message.packet.clone());
                }
            }
        }

        Ok(())
    }
    pub async fn send_reliable(&self, message: ServerMessage) -> Result<(), SessionSendError> {
        match message.recipient {
            Recipient::All => return Err(SessionSendError::Prohibited),
            Recipient::Single(pk) => {
                let tx = self
                    .get_channel(&pk)
                    .ok_or(SessionSendError::NoSuchSession)?;
                tx.send(message.packet).await?;
            }
            Recipient::List(pks) => {
                for pk in pks {
                    if let Some(tx) = self.get_channel(&pk) {
                        tx.send(message.packet.clone()).await?;
                    }
                }
            }
            Recipient::Except(_) => return Err(SessionSendError::Prohibited),
        }

        Ok(())
    }
}
