use shared::{ChatPacket, OutgoingPacket, PlayerKey};

use crate::{
    envelope::{EnvelopeRecipient, ServerEnvelope},
    runtime::{GameModeClientApi, protocol::GameModeClientCommand},
    socket::{
        protocol::{ServerMessage, SocketCommand},
        session_registry::SessionSender,
    },
};

#[derive(Clone)]
pub struct GameModeDefaultClientApi {
    pub ws_session_sender: SessionSender,
}
impl GameModeClientApi for GameModeDefaultClientApi {
    fn has(&self, pk: PlayerKey) -> bool {
        self.ws_session_sender.has_session(&pk)
    }

    fn list(&self) -> Vec<PlayerKey> {
        self.ws_session_sender.player_keys()
    }

    fn send(&self, event: GameModeClientCommand) {
        match event {
            GameModeClientCommand::SendMessage { pk, text } => {
                let _ = self.ws_session_sender.send_ephemeral(ServerMessage {
                    recipient: EnvelopeRecipient::Single(pk),
                    payload: OutgoingPacket::Chat(ChatPacket::GlobalMessage {
                        message: text,
                        color: String::from("#FFFFFF"),
                    }),
                });
            }
            GameModeClientCommand::KickPlayer { pk } => {
                let _ = self.ws_session_sender.send_control(ServerEnvelope {
                    recipient: EnvelopeRecipient::Single(pk),
                    payload: SocketCommand::Kick,
                });
            }
            GameModeClientCommand::Broadcast { text } => {}
            GameModeClientCommand::Log { text } => {}
        }
    }
}
