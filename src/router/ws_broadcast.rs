use shared::GameCommitEphemeral;
use tokio::sync::mpsc;

use crate::actor::ws::server_message::ServerMessage;

pub struct WsBroadcastRouter {
    pub broadcaster: mpsc::Sender<ServerMessage>,
}

impl WsBroadcastRouter {
    pub fn publish(&self, commit: GameCommitEphemeral) {
        match commit {
            GameCommitEphemeral::PlayerMoved { fid, x, y } => {
                let _ = self
                    .broadcaster
                    .send(ServerMessage::PlayerMoved { fid, x, y });
            }
        }
    }
}
