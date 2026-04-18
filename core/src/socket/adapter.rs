use std::collections::HashMap;

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
    pub query: HashMap<String, serde_json::Value>,
}
pub struct SocketAdapter {
    ws_tx: SplitSink<WebSocket, Message>,
    ws_rs: SplitStream<WebSocket>,
    session: Session,
    runtime_callback_tx: flume::Sender<RuntimeCallback>,
    query: HashMap<String, serde_json::Value>,
}
impl SocketAdapter {
    pub fn new(params: SocketAdapterParams) -> Self {
        let (ws_tx, ws_rs) = params.socket.split();
        Self {
            ws_tx,
            ws_rs,
            session: params.session,
            runtime_callback_tx: params.runtime_callback_tx,
            query: params.query,
        }
    }

    async fn process_request(&self, request: IncomingRequest) -> eyre::Result<()> {
        self.runtime_callback_tx
            .send_async(RuntimeCallback::Client(ClientEnvelope {
                sender: self.session.pk,
                payload: request,
            }))
            .await?;

        Ok(())
    }

    pub async fn activate(mut self) -> eyre::Result<()> {
        self.runtime_callback_tx
            .send_async(RuntimeCallback::System(SystemCallback::PlayerConnect {
                pk: self.session.pk,
                connection_params: self.query.clone(),
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
                                        Err(err) => {
                                            eprintln!("Unknown socket message: {}", err);
                                        }
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
            .send_async(RuntimeCallback::System(SystemCallback::PlayerDisconnect {
                pk: self.session.pk,
            }))
            .await?;
        Ok(())
    }
}
