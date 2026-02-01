use axum::extract::ws::{Message, WebSocket};
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use shared::{IncomingRequest, OutgoingPacket, SystemPacket};
use tokio::sync::{broadcast::error::RecvError, mpsc};

use crate::{
    envelope::ClientEnvelope,
    socket::{
        protocol::SocketCommand,
        session_registry::{Session, protocol::SessionCommand},
    },
};

pub struct SocketAdapter {
    ws_tx: SplitSink<WebSocket, Message>,
    ws_rs: SplitStream<WebSocket>,
    session: Session,
    client_messenger_tx: mpsc::Sender<ClientEnvelope<IncomingRequest>>,
}
impl SocketAdapter {
    pub fn new(
        socket: WebSocket,
        session: Session,
        client_messenger_tx: mpsc::Sender<ClientEnvelope<IncomingRequest>>,
    ) -> Self {
        let (ws_tx, ws_rs) = socket.split();
        Self {
            ws_tx,
            ws_rs,
            session,
            client_messenger_tx,
        }
    }

    pub async fn activate(mut self) {
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
                                        Ok(payload) => {
                                            let _ = self.client_messenger_tx.send(ClientEnvelope {
                                                sender: self.session.pk,
                                                payload
                                            }).await;
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
    }
}
