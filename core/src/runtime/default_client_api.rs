use shared::PlayerKey;

use crate::{
    runtime::GameModeClientApi,
    socket::{protocol::ServerMessage, session_registry::SessionSender},
};

#[derive(Clone)]
pub struct GameModeDefaultClientApi {
    pub ws_session_sender: SessionSender,
}
impl GameModeClientApi for GameModeDefaultClientApi {
    fn has(&self, pk: PlayerKey) -> bool {
        self.ws_session_sender.has_session(pk)
    }

    fn list(&self) -> Vec<PlayerKey> {
        self.ws_session_sender.player_keys()
    }

    fn send(&self, message: ServerMessage) {
        let _ = self.ws_session_sender.send_message(message);
    }

    fn identity(&self, pk: PlayerKey) -> Option<String> {
        self.ws_session_sender.get_identity(pk)
    }
}
