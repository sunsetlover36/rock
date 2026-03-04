use crate::{router::CommitKind, socket::session_registry::SessionSender};

pub struct WsCommitRouter {
    pub ws_session_sender: SessionSender,
}
impl WsCommitRouter {
    pub fn publish(&self, commit: CommitKind) {
        match commit {}
    }
}
