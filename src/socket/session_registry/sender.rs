use std::sync::Arc;

use shared::PlayerKey;
use tokio::sync::mpsc;

use crate::{
    envelope::EnvelopeRecipient,
    socket::{
        protocol::{ControlMessage, ServerMessage},
        session_registry::protocol::{SessionCommand, SessionRegistryState, SessionSendError},
    },
};

#[derive(Clone)]
pub struct SessionSender {
    pub(super) inner: Arc<SessionRegistryState>,
    pub(super) tokio_handle: tokio::runtime::Handle,
}
impl SessionSender {
    fn get_endpoint(&self, pk: &PlayerKey) -> Option<mpsc::Sender<SessionCommand>> {
        self.inner.sessions.get(pk).map(|e| e.value().clone())
    }

    pub fn send_message(&self, message: ServerMessage) {
        match message.recipient {
            EnvelopeRecipient::All => {
                let _ = self.inner.broadcast_hub.send(message.payload);
            }
            EnvelopeRecipient::Single(pk) => {
                if let Some(tx) = self.get_endpoint(&pk) {
                    let _ = tx.try_send(SessionCommand::Data(message.payload));
                }
            }
            EnvelopeRecipient::List(pks) => {
                for pk in pks {
                    if let Some(tx) = self.get_endpoint(&pk) {
                        let _ = tx.try_send(SessionCommand::Data(message.payload.clone()));
                    }
                }
            }
            EnvelopeRecipient::Except(except_pk) => {
                for entry in self.inner.sessions.iter() {
                    let pk = *entry.key();
                    if pk == except_pk {
                        continue;
                    }

                    let tx = entry.value().clone();
                    let _ = tx.try_send(SessionCommand::Data(message.payload.clone()));
                }
            }
        }
    }

    pub fn send_control(
        &self,
        command: ControlMessage,
    ) -> Result<(), SessionSendError<SessionCommand>> {
        match command.recipient {
            EnvelopeRecipient::Single(pk) => {
                let tx = self
                    .get_endpoint(&pk)
                    .ok_or(SessionSendError::NoSuchSession)?;
                let payload = SessionCommand::Control(command.payload);

                match tx.try_send(payload.clone()) {
                    Ok(_) => Ok(()),
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        Err(SessionSendError::ChannelClosed(payload))
                    }
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        let tx = tx.clone();
                        self.tokio_handle.spawn(async move {
                            let _ = tx.send(payload).await;
                        });

                        Ok(())
                    }
                }
            }
            _ => Err(SessionSendError::Prohibited),
        }
    }

    pub fn has_session(&self, pk: &PlayerKey) -> bool {
        self.inner.sessions.contains_key(pk)
    }
    pub fn player_keys(&self) -> Vec<PlayerKey> {
        self.inner.sessions.iter().map(|e| *e.key()).collect()
    }
}
