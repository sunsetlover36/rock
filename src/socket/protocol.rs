use shared::{OutgoingPacket, PlayerKey};

#[derive(Debug, Clone)]
pub enum Recipient {
    All,
    Single(PlayerKey),
    List(Vec<PlayerKey>),
    Except(PlayerKey),
}

// Temporarily deprecated
#[derive(Debug, Clone, Copy)]
pub enum Delivery {
    Ephemeral,
    Reliable,
}

#[derive(Debug, Clone)]
pub struct ServerMessage {
    pub recipient: Recipient,
    pub packet: OutgoingPacket,
}
