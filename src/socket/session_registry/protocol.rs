use dashmap::DashMap;
use rock_wire::{OutgoingPacket, PlayerKey};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
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

#[derive(Debug, Default)]
pub(crate) struct SessionBackpressureStats {
    private_channel_full: AtomicU64,
    private_channel_closed: AtomicU64,
    broadcast_lagged_events: AtomicU64,
    broadcast_lagged_packets: AtomicU64,
}
impl SessionBackpressureStats {
    fn should_log(count: u64) -> bool {
        count.is_power_of_two()
    }

    pub fn record_private_channel_full(&self, pk: PlayerKey) {
        let count = self.private_channel_full.fetch_add(1, Ordering::Relaxed) + 1;
        if Self::should_log(count) {
            eprintln!(
                "[WS] Dropped outgoing packet for player {:?}: private session channel is full (total dropped: {})",
                pk, count
            );
        }
    }

    pub fn record_private_channel_closed(&self, pk: PlayerKey) {
        let count = self.private_channel_closed.fetch_add(1, Ordering::Relaxed) + 1;
        if Self::should_log(count) {
            eprintln!(
                "[WS] Dropped outgoing packet for player {:?}: private session channel is closed (total closed-channel drops: {})",
                pk, count
            );
        }
    }

    pub fn record_broadcast_lagged(&self, pk: PlayerKey, skipped: u64) {
        let events = self.broadcast_lagged_events.fetch_add(1, Ordering::Relaxed) + 1;
        let skipped_total = self
            .broadcast_lagged_packets
            .fetch_add(skipped, Ordering::Relaxed)
            + skipped;

        if Self::should_log(events) {
            eprintln!(
                "[WS] Broadcast receiver lagged for player {:?}: skipped {} packet(s) (lag events: {}, total skipped: {})",
                pk, skipped, events, skipped_total
            );
        }
    }
}

pub(crate) struct PlayerConnection {
    pub pk: PlayerKey,
    pub identity: Option<String>,
    pub session_rx: mpsc::Receiver<SessionCommand>,
    pub broadcast_rx: broadcast::Receiver<OutgoingPacket>,
    pub stats: Arc<SessionBackpressureStats>,
    registrar: SessionRegistrar,
}
impl PlayerConnection {
    pub fn new(
        pk: PlayerKey,
        identity: Option<String>,
        session_rx: mpsc::Receiver<SessionCommand>,
        broadcast_rx: broadcast::Receiver<OutgoingPacket>,
        stats: Arc<SessionBackpressureStats>,
        registrar: SessionRegistrar,
    ) -> Self {
        Self {
            pk,
            identity,
            session_rx,
            broadcast_rx,
            stats,
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
    pub stats: Arc<SessionBackpressureStats>,
}
