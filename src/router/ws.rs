use shared::{OutgoingPacket, WorldPacket};

use crate::{
    envelope::{EnvelopeRecipient, ServerEnvelope},
    socket::session_registry::SessionSender,
    world::protocol::WorldCommit,
};

pub struct WsCommitRouter {
    pub ws_session_sender: SessionSender,
}
impl WsCommitRouter {
    pub fn publish(&self, commit: WorldCommit) {
        match commit {
            WorldCommit::PlayerMoved { fid, x, y } => {
                let _ = self.ws_session_sender.send_ephemeral(ServerEnvelope {
                    recipient: EnvelopeRecipient::All,
                    payload: OutgoingPacket::World(WorldPacket::PlayerMoved { fid, x, y }),
                });
            }
            WorldCommit::BiomeExplored => {}
        }
    }
}
