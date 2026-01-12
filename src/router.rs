mod ws_broadcast;
use tokio::sync::mpsc;
use ws_broadcast::WsBroadcastRouter;

use shared::{OutgoingPacket, WorldCommit};

pub struct CommitRouter {
    // db: DatabaseSystem
    // logger: LoggerSystem
    ws_broadcast: WsBroadcastRouter,
}

impl CommitRouter {
    pub fn new(ws_broadcast_tx: mpsc::Sender<OutgoingPacket>) -> Self {
        Self {
            ws_broadcast: WsBroadcastRouter { ws_broadcast_tx },
        }
    }
    pub fn emit(&self, commit: WorldCommit) {
        self.ws_broadcast.publish(commit);
    }
}
