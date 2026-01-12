use std::sync::Arc;

use crate::{
    actor::gamemode::types::{GameModeEvent, GameModeEventListener},
    socket::session_registry::SessionRegistry,
};

pub struct GameModeDefaultEventListener {
    pub session_registry: Arc<SessionRegistry>,
}

#[async_trait::async_trait]
impl GameModeEventListener for GameModeDefaultEventListener {
    async fn on_emit(&self, event: GameModeEvent) {
        match event {
            GameModeEvent::SendClientMessage { pk, text } => {}
            GameModeEvent::Broadcast { text } => {}
            GameModeEvent::Log { text } => {}
        }
    }
}
