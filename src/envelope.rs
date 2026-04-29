// Global client protocol (transport-agnostic)
//
// ClientEnvelope<T>
// Ties a sender (player key) with the payload (any message)
// Untrusted by the nature
//
// ServerEnvelope<T>
// Deliver any message to a single recipient or a list of recipients (EnvelopeRecipient)
// Can be trusted (constructed by the engine)

use rock_wire::PlayerKey;

#[derive(Debug, Clone)]
pub struct ClientEnvelope<T> {
    pub sender: PlayerKey,
    pub payload: T,
}

#[derive(Debug, Clone)]
pub enum EnvelopeRecipient {
    All,
    Single(PlayerKey),
    List(Vec<PlayerKey>),
    Except(PlayerKey),
}

#[derive(Debug, Clone)]
pub struct ServerEnvelope<T> {
    pub recipient: EnvelopeRecipient,
    pub payload: T,
}
