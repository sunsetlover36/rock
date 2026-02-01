use dashmap::DashMap;
use shared::{OutgoingPacket, PlayerKey};
use tokio::sync::{broadcast, mpsc};

use crate::{player_pool::PlayerPool, socket::protocol::SocketCommand};

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
pub enum SessionCommand {
    Data(OutgoingPacket),
    Control(SocketCommand),
}

pub struct SessionRegistryState {
    pub player_pool: parking_lot::Mutex<PlayerPool>,
    pub broadcast_hub: broadcast::Sender<OutgoingPacket>,
    pub session_channel_buffer: usize,
    pub sessions: DashMap<PlayerKey, mpsc::Sender<SessionCommand>>,
}
