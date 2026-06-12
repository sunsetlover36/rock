use axum::extract::ws::{Message, WebSocket};
use color_eyre::eyre;
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use rock_wire::{IncomingRequest, OutgoingPacket, SocketConnectionQuery, SystemPacket};
use serde::Serialize;
use tokio::{
    sync::broadcast::error::RecvError,
    time::{self, Duration},
};

use crate::{
    envelope::ClientEnvelope,
    runtime::{RuntimeCallback, SystemCallback},
    socket::{
        protocol::SocketCommand,
        session_registry::protocol::{PlayerConnection, SessionCommand},
    },
};

pub struct SocketAdapterParams {
    pub socket: WebSocket,
    pub session: PlayerConnection,
    pub runtime_callback_tx: flume::Sender<RuntimeCallback>,
    pub query: SocketConnectionQuery,
}
pub struct SocketAdapter {
    ws_tx: SplitSink<WebSocket, Message>,
    ws_rs: SplitStream<WebSocket>,
    session: PlayerConnection,
    runtime_callback_tx: flume::Sender<RuntimeCallback>,
    query: SocketConnectionQuery,
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

    async fn send(&mut self, data: &impl Serialize) {
        match serde_json::to_string(data) {
            Ok(json) => {
                let _ = self.ws_tx.send(Message::Text(json.into())).await;
            }
            Err(err) => {
                eprintln!("Failed to serialize a session packet: {err}");
            }
        }
    }

    pub async fn activate(mut self) -> eyre::Result<()> {
        self.runtime_callback_tx
            .send_async(RuntimeCallback::System(SystemCallback::PlayerConnect {
                pk: self.session.pk,
                connection_params: self.query.clone(),
            }))
            .await?;

        let mut ping_interval = time::interval(Duration::from_secs(25));
        loop {
            tokio::select! {
                _ = ping_interval.tick() => {
                    if self.ws_tx.send(Message::Ping(Vec::new().into())).await.is_err() {
                        break;
                    }
                }

                Some(command) = self.session.session_rx.recv() => {
                    match command {
                        SessionCommand::Data(packet) => {
                            self.send(&packet).await;
                        },
                        SessionCommand::Control(command) => {
                            match command {
                                SocketCommand::Kick => {
                                    let p = OutgoingPacket::System(SystemPacket::PlayerKicked);
                                    self.send(&p).await;

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
                            self.send(&p).await;
                        }
                        Err(RecvError::Lagged(skipped)) => {
                            self.session
                                .stats
                                .record_broadcast_lagged(self.session.pk, skipped);
                        }
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
                identity: self.session.identity.clone(),
            }))
            .await?;
        Ok(())
    }
}
