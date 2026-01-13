use shared::{ChatPacket, OutgoingPacket, Recipient, ServerMessage};

use crate::{
    actor::gamemode::types::{GameModeEvent, GameModeEventListener},
    socket::session_registry::SessionSender,
};

pub struct GameModeDefaultEventListener {
    pub ws_session_sender: SessionSender,
}

impl GameModeEventListener for GameModeDefaultEventListener {
    fn on_emit(&self, event: GameModeEvent) {
        match event {
            GameModeEvent::SendClientMessage { pk, text } => {
                let _ = self.ws_session_sender.send_ephemeral(ServerMessage {
                    recipient: Recipient::Single(pk),
                    packet: OutgoingPacket::Chat(ChatPacket::Message {
                        message: text,
                        color: String::from("#FFFFFF"),
                    }),
                });
            }
            GameModeEvent::Broadcast { text } => {}
            GameModeEvent::Log { text } => {}
        }
    }
}
