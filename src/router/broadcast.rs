use shared::GameCommitEphemeral;
use tokio::sync::broadcast;

use crate::actor::ws::server_message::ServerMessage;

pub struct WsBroadcastRouter {
    propagator_tx: broadcast::Sender<ServerMessage>,
}

impl WsBroadcastRouter {
    pub fn publish(&self, commit: GameCommitEphemeral) {
        match commit {
            GameCommitEphemeral::PlayerMoved { fid, x, y } => {
                let _ = self
                    .propagator_tx
                    .send(ServerMessage::PlayerMoved { fid, x, y });
            }
        }
    }
}
