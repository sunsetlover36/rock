mod ws_broadcast;
use tokio::sync::mpsc;
use ws_broadcast::WsBroadcastRouter;

use shared::GameCommit;

use crate::actor::ws::server_message::ServerMessage;

pub struct CommitRouter {
    // db: DatabaseSystem
    // logger: LoggerSystem
    ws_broadcast: WsBroadcastRouter,
}

impl CommitRouter {
    pub fn new(ws_broadcaster: mpsc::Sender<ServerMessage>) -> Self {
        Self {
            ws_broadcast: WsBroadcastRouter {
                broadcaster: ws_broadcaster,
            },
        }
    }
    pub fn emit(&self, commit: GameCommit) {
        match commit {
            GameCommit::Ephemeral(e) => {
                self.ws_broadcast.publish(e);
            }
            GameCommit::Durable(d) => {
                // Every method here should be sync, not async (don't block the execution of this thread)
            }
        }
    }
}
