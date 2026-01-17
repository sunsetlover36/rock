use shared::{ChatPacket, OutgoingPacket};

use crate::{
    actor::gamemode::{GameModeEventListener, protocol::GameModeEvent},
    envelope::EnvelopeRecipient,
    envelope::ServerEnvelope,
    socket::{
        protocol::{ServerMessage, SocketCommand},
        session_registry::SessionSender,
    },
};

pub struct GameModeDefaultEventListener {
    pub ws_session_sender: SessionSender,
}

impl GameModeEventListener for GameModeDefaultEventListener {
    fn on_emit(&self, event: GameModeEvent) {
        match event {
            GameModeEvent::SendClientMessage { pk, text } => {
                let _ = self.ws_session_sender.send_ephemeral(ServerMessage {
                    recipient: EnvelopeRecipient::Single(pk),
                    payload: OutgoingPacket::Chat(ChatPacket::GlobalMessage {
                        message: text,
                        color: String::from("#FFFFFF"),
                    }),
                });
            }
            GameModeEvent::KickPlayer { pk } => {
                let _ = self.ws_session_sender.send_control_command(ServerEnvelope {
                    recipient: EnvelopeRecipient::Single(pk),
                    payload: SocketCommand::Kick,
                });
            }
            GameModeEvent::Broadcast { text } => {}
            GameModeEvent::Log { text } => {}
        }
    }
}
