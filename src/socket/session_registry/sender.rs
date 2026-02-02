use std::sync::Arc;

use shared::PlayerKey;
use tokio::sync::mpsc;

use crate::{
    envelope::EnvelopeRecipient,
    socket::{
        protocol::{ServerMessage, SocketControl},
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

    // Cases:
    // -> 1. message.delivery == Reliable and message.recipient == All -> Prohibited (can't guarantee it right now)
    // -> 2. message.delivery == Ephemeral and message.recipient == All -> Sync [broadcast]
    // -> 3. message.delivery == Reliable and message.recipient != All -> Async [unicast: send().await]
    // -> 4. message.delivery == Ephemeral and message.recipient != All -> Sync [unicast: try_send()]
    // -> 5. message.delivery == Reliable and message.recipient == Except -> Prohibited (can't guarantee it right now)
    //
    // TODO: Add a timeout for slow consumers with a forced disconnection opportunity
    // -> To prevent blocking a thread
    // -> Required for a reliable message delivery to multiple recipients
    pub fn send_ephemeral(&self, message: ServerMessage) {
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

    pub async fn send_reliable(
        &self,
        message: ServerMessage,
    ) -> Result<(), SessionSendError<SessionCommand>> {
        match message.recipient {
            EnvelopeRecipient::All => return Err(SessionSendError::Prohibited),
            EnvelopeRecipient::Single(pk) => {
                let tx = self
                    .get_endpoint(&pk)
                    .ok_or(SessionSendError::NoSuchSession)?;
                tx.send(SessionCommand::Data(message.payload)).await?;
            }
            EnvelopeRecipient::List(pks) => {
                for pk in pks {
                    if let Some(tx) = self.get_endpoint(&pk) {
                        tx.send(SessionCommand::Data(message.payload.clone()))
                            .await?;
                    }
                }
            }
            EnvelopeRecipient::Except(_) => return Err(SessionSendError::Prohibited),
        }

        Ok(())
    }

    pub fn send_control_command(
        &self,
        command: SocketControl,
    ) -> Result<(), SessionSendError<SessionCommand>> {
        match command.recipient {
            EnvelopeRecipient::Single(pk) => {
                let tx = self
                    .get_endpoint(&pk)
                    .ok_or(SessionSendError::NoSuchSession)?;
                let payload = SessionCommand::Control(command.payload);

                match tx.try_send(payload.clone()) {
                    Ok(_) => return Ok(()),
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        return Err(SessionSendError::ChannelClosed(payload));
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
}
