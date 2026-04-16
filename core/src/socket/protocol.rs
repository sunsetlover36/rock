use shared::OutgoingPacket;

use crate::envelope::ServerEnvelope;

#[derive(Debug, Clone)]
pub enum SocketCommand {
    Kick,
}

pub type ServerMessage = ServerEnvelope<OutgoingPacket>;
pub type ControlMessage = ServerEnvelope<SocketCommand>;
