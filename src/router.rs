mod ws;
use ws::WsCommitRouter;

use crate::{socket::session_registry::SessionSender, world::protocol::WorldCommit};

pub struct CommitRouter {
    // db: DatabaseSystem
    // logger: LoggerSystem
    ws_session_sender: WsCommitRouter,
}

impl CommitRouter {
    pub fn new(ws_session_sender: SessionSender) -> Self {
        Self {
            ws_session_sender: WsCommitRouter { ws_session_sender },
        }
    }
    pub fn emit(&self, commit: WorldCommit) {
        self.ws_session_sender.publish(commit);
    }
}
