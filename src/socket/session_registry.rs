// session_registry.rs
// A unified WebSocket session and delivery layer

use dashmap::DashMap;
use shared::{Delivery, OutgoingPacket, PlayerKey, Recipient, ServerMessage};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

use crate::player_pool::PlayerPool;

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

pub struct SessionRegistry {
    player_pool: parking_lot::Mutex<PlayerPool>,
    ws_channel_buffer: usize,
    ws_broadcast_hub: broadcast::Sender<OutgoingPacket>,
    inner: DashMap<PlayerKey, mpsc::Sender<OutgoingPacket>>,
}
impl SessionRegistry {
    pub fn new(
        ws_channel_buffer: usize,
        ws_broadcast_hub: broadcast::Sender<OutgoingPacket>,
        player_pool: PlayerPool,
    ) -> Self {
        let sessions = DashMap::new();

        Self {
            player_pool: parking_lot::Mutex::new(player_pool),
            ws_channel_buffer,
            ws_broadcast_hub,
            inner: sessions,
        }
    }

    pub fn register(
        &self,
    ) -> (
        PlayerKey,
        mpsc::Receiver<OutgoingPacket>,
        broadcast::Receiver<OutgoingPacket>,
    ) {
        let pk = {
            let mut pool = self.player_pool.lock();
            pool.claim()
        };
        let (tx, rx) = mpsc::channel::<OutgoingPacket>(self.ws_channel_buffer);

        self.inner.insert(pk, tx);
        (pk, rx, self.ws_broadcast_hub.subscribe())
    }
    pub fn unregister(&self, pk: &PlayerKey) {
        self.inner.remove(pk);
        self.player_pool.lock().release(pk);
    }

    fn get_channel(&self, pk: &PlayerKey) -> Option<mpsc::Sender<OutgoingPacket>> {
        self.inner.get(pk).map(|tx| tx.value().clone())
    }

    pub async fn send(&self, message: ServerMessage) -> Result<(), SessionSendError> {
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
        match message.delivery {
            Delivery::Ephemeral => match message.recipient {
                Recipient::All => self.broadcast_tx.try_send(message.packet),
                Recipient::Single(pk) => {
                    let tx = self
                        .get_channel(&pk)
                        .ok_or(SessionSendError::NoSuchSession)?;
                    tx.try_send(message.packet);
                }
                Recipient::List(pks) => {
                    for pk in pks {
                        if let Some(tx) = self.get_channel(&pk) {
                            tx.try_send(message.packet.clone());
                        }
                    }
                }
                Recipient::Except(except_pk) => {
                    for entry in self.inner.iter() {
                        let pk = *entry.key();
                        if pk == except_pk {
                            continue;
                        }

                        let tx = entry.value().clone();
                        tx.try_send(message.packet.clone());
                    }
                }
            },
            Delivery::Reliable => match message.recipient {
                Recipient::All => Err(SessionSendError::Prohibited),
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
                Recipient::Except(except_pk) => Err(SessionSendError::Prohibited),
            },
        }
    }
}

pub type SharedSessionRegistry = Arc<SessionRegistry>;
