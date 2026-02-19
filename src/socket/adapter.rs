use axum::extract::ws::{Message, WebSocket};
use color_eyre::eyre;
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use shared::{IncomingRequest, OutgoingPacket, SystemPacket};
use tokio::sync::broadcast::error::RecvError;

use crate::{
    envelope::ClientEnvelope,
    runtime::{RuntimeCallback, SystemCallback},
    socket::{
        protocol::SocketCommand,
        session_registry::{Session, protocol::SessionCommand},
    },
};

pub struct SocketAdapterParams {
    pub socket: WebSocket,
    pub session: Session,
    pub runtime_callback_tx: flume::Sender<RuntimeCallback>,
}
pub struct SocketAdapter {
    ws_tx: SplitSink<WebSocket, Message>,
    ws_rs: SplitStream<WebSocket>,
    session: Session,
    runtime_callback_tx: flume::Sender<RuntimeCallback>,
}
impl SocketAdapter {
    pub fn new(params: SocketAdapterParams) -> Self {
        let (ws_tx, ws_rs) = params.socket.split();
        Self {
            ws_tx,
            ws_rs,
            session: params.session,
            runtime_callback_tx: params.runtime_callback_tx,
        }
    }

    async fn process_request(&self, request: IncomingRequest) -> eyre::Result<()> {
        match request {
            IncomingRequest::GameMode(payload) => {
                self.runtime_callback_tx
                    .send_async(RuntimeCallback::Client(ClientEnvelope {
                        sender: self.session.pk,
                        payload,
                    }))
                    .await?;
            }
        }

        Ok(())
    }

    pub async fn activate(mut self) -> eyre::Result<()> {
        self.runtime_callback_tx
            .send_async(RuntimeCallback::System(SystemCallback::OnPlayerConnect {
                pk: self.session.pk,
            }))
            .await?;

        loop {
            tokio::select! {
                biased;

                Some(command) = self.session.session_rx.recv() => {
                    match command {
                        SessionCommand::Data(packet) => {
                            // TODO: Need an util for JSON convertation and send
                            let json = serde_json::to_string(&packet).unwrap();
                            let _ = self.ws_tx.send(Message::Text(json.into())).await;
                        },
                        SessionCommand::Control(command) => {
                            match command {
                                SocketCommand::Kick => {
                                    let p = OutgoingPacket::System(SystemPacket::PlayerKicked);
                                    let json = serde_json::to_string(&p).unwrap();

                                    // best-effort UX
                                    let _ = self.ws_tx.send(Message::Text(json.into())).await;

                                    // force disconnect
                                    let _ = self.ws_tx.send(Message::Close(None)).await;
                                    break;
                                }
                            }
                        }
                    }
                }

                res = self.session.broadcast_rx.recv() => {
                    match res {
                        Ok(p) => {
                            let json = serde_json::to_string(&p).unwrap();
                            let _ = self.ws_tx.send(Message::Text(json.into())).await;
                        }
                        Err(RecvError::Lagged(_)) => {}
                        Err(RecvError::Closed) => {
                            break;
                        }
                    }
                }

                ws = self.ws_rs.next() => {
                    match ws {
                        Some(Ok(msg)) => {
                            match msg {
                                Message::Text(text) => {
                                    match serde_json::from_str::<IncomingRequest>(&text) {
                                        Ok(request) => {
                                            self.process_request(request).await?;
                                        }
                                        Err(_) => {}
                                    }
                                }
                                Message::Close(_) => {
                                    break;
                                }
                                _ => {}
                            }
                        }
                        Some(Err(_)) | None => break,
                    }
                }
            }
        }

        self.runtime_callback_tx
            .send_async(RuntimeCallback::System(
                SystemCallback::OnPlayerDisconnect {
                    pk: self.session.pk,
                },
            ))
            .await?;
        Ok(())
    }
}
