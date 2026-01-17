// session_registry.rs
// A unified WebSocket session and delivery layer

use dashmap::DashMap;
use shared::{OutgoingPacket, PlayerKey};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

use crate::{
    envelope::EnvelopeRecipient,
    player_pool::PlayerPool,
    socket::protocol::{ServerMessage, SocketCommand, SocketControl},
};

#[derive(Debug)]
pub enum SessionSendError<T> {
    NoSuchSession,
    Prohibited,
    ChannelFull(T),
    ChannelClosed(T),
}
impl<T> From<mpsc::error::TrySendError<T>> for SessionSendError<T> {
    fn from(err: mpsc::error::TrySendError<T>) -> Self {
        match err {
            mpsc::error::TrySendError::Full(p) => SessionSendError::ChannelFull(p),
            mpsc::error::TrySendError::Closed(p) => SessionSendError::ChannelClosed(p),
        }
    }
}
impl<T> From<mpsc::error::SendError<T>> for SessionSendError<T> {
    fn from(err: mpsc::error::SendError<T>) -> Self {
        SessionSendError::ChannelClosed(err.0)
    }
}

#[derive(Debug, Clone)]
pub struct SessionEndpoint {
    unicast: mpsc::Sender<OutgoingPacket>,
    control: mpsc::Sender<SocketCommand>,
}
struct RegistryInner {
    player_pool: parking_lot::Mutex<PlayerPool>,
    broadcast_hub: broadcast::Sender<OutgoingPacket>,
    unicast_channel_buffer: usize,
    control_channel_buffer: usize,
    sessions: DashMap<PlayerKey, SessionEndpoint>,
}
pub struct SessionRegistry {
    inner: Arc<RegistryInner>,
}
impl SessionRegistry {
    pub fn new(
        broadcast_hub_buffer: usize,
        unicast_channel_buffer: usize,
        control_channel_buffer: usize,
        player_pool: PlayerPool,
    ) -> Self {
        let sessions = DashMap::new();
        let (broadcast_hub, _) = broadcast::channel::<OutgoingPacket>(broadcast_hub_buffer);

        Self {
            inner: Arc::new(RegistryInner {
                player_pool: parking_lot::Mutex::new(player_pool),
                broadcast_hub,
                unicast_channel_buffer,
                control_channel_buffer,
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
    pub control_rx: mpsc::Receiver<SocketCommand>,
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
        let (uni_tx, uni_rx) = mpsc::channel::<OutgoingPacket>(self.inner.unicast_channel_buffer);
        let (control_tx, control_rx) =
            mpsc::channel::<SocketCommand>(self.inner.control_channel_buffer);

        self.inner.sessions.insert(
            pk,
            SessionEndpoint {
                unicast: uni_tx,
                control: control_tx,
            },
        );
        Session {
            pk,
            unicast_rx: uni_rx,
            broadcast_rx: self.inner.broadcast_hub.subscribe(),
            control_rx,
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
    fn get_endpoints(&self, pk: &PlayerKey) -> Option<SessionEndpoint> {
        self.inner.sessions.get(pk).map(|e| e.value().clone())
    }
    fn get_unicast(&self, pk: &PlayerKey) -> Option<mpsc::Sender<OutgoingPacket>> {
        if let Some(e) = self.get_endpoints(pk) {
            return Some(e.unicast);
        }

        None
    }
    fn get_control(&self, pk: &PlayerKey) -> Option<mpsc::Sender<SocketCommand>> {
        if let Some(e) = self.get_endpoints(pk) {
            return Some(e.control);
        }

        None
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
    pub fn send_ephemeral(&self, message: ServerMessage) {
        match message.recipient {
            EnvelopeRecipient::All => {
                let _ = self.inner.broadcast_hub.send(message.payload);
            }
            EnvelopeRecipient::Single(pk) => {
                if let Some(tx) = self.get_unicast(&pk) {
                    let _ = tx.try_send(message.payload);
                }
            }
            EnvelopeRecipient::List(pks) => {
                for pk in pks {
                    if let Some(tx) = self.get_unicast(&pk) {
                        let _ = tx.try_send(message.payload.clone());
                    }
                }
            }
            EnvelopeRecipient::Except(except_pk) => {
                for entry in self.inner.sessions.iter() {
                    let pk = *entry.key();
                    if pk == except_pk {
                        continue;
                    }

                    let tx = entry.value().clone();
                    let _ = tx.unicast.try_send(message.payload.clone());
                }
            }
        }
    }
    pub async fn send_reliable(
        &self,
        message: ServerMessage,
    ) -> Result<(), SessionSendError<OutgoingPacket>> {
        match message.recipient {
            EnvelopeRecipient::All => return Err(SessionSendError::Prohibited),
            EnvelopeRecipient::Single(pk) => {
                let tx = self
                    .get_unicast(&pk)
                    .ok_or(SessionSendError::NoSuchSession)?;
                tx.send(message.payload).await?;
            }
            EnvelopeRecipient::List(pks) => {
                for pk in pks {
                    if let Some(tx) = self.get_unicast(&pk) {
                        tx.send(message.payload.clone()).await?;
                    }
                }
            }
            EnvelopeRecipient::Except(_) => return Err(SessionSendError::Prohibited),
        }

        Ok(())
    }

    pub fn send_control_command(
        &self,
        command: SocketControl,
    ) -> Result<(), SessionSendError<SocketCommand>> {
        match command.recipient {
            EnvelopeRecipient::Single(pk) => {
                let tx = self
                    .get_control(&pk)
                    .ok_or(SessionSendError::NoSuchSession)?;

                tx.try_send(command.payload)?;
                Ok(())
            }
            _ => Err(SessionSendError::Prohibited),
        }
    }
}
