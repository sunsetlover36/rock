// Global client protocol (transport-agnostic)
// Ties a sender (player key) with the payload (any message)
// Untrusted by the nature

use shared::PlayerKey;

#[derive(Debug, Clone)]
pub struct Envelope<T> {
    pub sender: PlayerKey,
    pub payload: T,
}
