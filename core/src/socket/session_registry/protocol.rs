use dashmap::DashMap;
use shared::{OutgoingPacket, PlayerKey};
use tokio::sync::{broadcast, mpsc};

use crate::{
    player_pool::PlayerPool,
    socket::{protocol::SocketCommand, session_registry::SessionRegistrar},
};

#[derive(Debug)]
pub(crate) enum SessionSendError<T> {
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
pub(crate) enum SessionCommand {
    Data(OutgoingPacket),
    Control(SocketCommand),
}

pub(crate) struct PlayerConnection {
    pub pk: PlayerKey,
    pub session_rx: mpsc::Receiver<SessionCommand>,
    pub broadcast_rx: broadcast::Receiver<OutgoingPacket>,
    registrar: SessionRegistrar,
}
impl PlayerConnection {
    pub fn new(
        pk: PlayerKey,
        session_rx: mpsc::Receiver<SessionCommand>,
        broadcast_rx: broadcast::Receiver<OutgoingPacket>,
        registrar: SessionRegistrar,
    ) -> Self {
        Self {
            pk,
            session_rx,
            broadcast_rx,
            registrar,
        }
    }
}
impl Drop for PlayerConnection {
    fn drop(&mut self) {
        self.registrar.unregister(&self.pk);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Session {
    pub identity: Option<String>,
    pub tx: mpsc::Sender<SessionCommand>,
}
pub struct SessionRegistryState {
    pub player_pool: parking_lot::Mutex<PlayerPool>,
    pub broadcast_hub: broadcast::Sender<OutgoingPacket>,
    pub session_channel_buffer: usize,
    pub sessions: DashMap<PlayerKey, Session>,
}
