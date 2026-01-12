use shared::{OutgoingPacket, WorldCommit, WorldPacket};
use tokio::sync::mpsc;

pub struct WsBroadcastRouter {
    pub ws_broadcast_tx: mpsc::Sender<OutgoingPacket>,
}

impl WsBroadcastRouter {
    pub fn publish(&self, commit: WorldCommit) {
        match commit {
            WorldCommit::PlayerMoved { fid, x, y } => {
                let _ =
                    self.ws_broadcast_tx
                        .send(OutgoingPacket::World(WorldPacket::PlayerMoved {
                            fid,
                            x,
                            y,
                        }));
            }
            WorldCommit::BiomeExplored => {}
        }
    }
}
