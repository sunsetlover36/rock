use shared::{OutgoingPacket, WorldPacket};

use crate::{
    socket::{
        protocol::{Recipient, ServerMessage},
        session_registry::SessionSender,
    },
    world::WorldCommit,
};

pub struct WsCommitRouter {
    pub ws_session_sender: SessionSender,
}

impl WsCommitRouter {
    pub fn publish(&self, commit: WorldCommit) {
        match commit {
            WorldCommit::PlayerMoved { fid, x, y } => {
                let _ = self.ws_session_sender.send_ephemeral(ServerMessage {
                    recipient: Recipient::All,
                    packet: OutgoingPacket::World(WorldPacket::PlayerMoved { fid, x, y }),
                });
            }
            WorldCommit::BiomeExplored => {}
        }
    }
}
