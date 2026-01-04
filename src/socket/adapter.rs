use axum::extract::ws::{Message, WebSocket};
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use tokio::sync::{
    broadcast::{self, error::RecvError},
    mpsc,
};

use crate::actor::ws::{client_message::ClientMessage, server_message::ServerMessage};

pub struct SocketAdapter {
    ws_tx: SplitSink<WebSocket, Message>,
    ws_rs: SplitStream<WebSocket>,
    server_message_rx: broadcast::Receiver<ServerMessage>,
    client_messenger_tx: mpsc::Sender<ClientMessage>,
}
impl SocketAdapter {
    pub fn new(
        socket: WebSocket,
        server_message_rx: broadcast::Receiver<ServerMessage>,
        client_messenger_tx: mpsc::Sender<ClientMessage>,
    ) -> Self {
        let (ws_tx, ws_rs) = socket.split();
        Self {
            ws_tx,
            ws_rs,
            server_message_rx,
            client_messenger_tx,
        }
    }

    pub async fn activate(mut self) {
        loop {
            tokio::select! {
                res = self.server_message_rx.recv() => {
                    match res {
                        Ok(msg) => {
                            let json = serde_json::to_string(&msg).unwrap();
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
                                    match serde_json::from_str::<ClientMessage>(&text) {
                                        Ok(client_msg) => {
                                            let _ = self.client_messenger_tx.send(client_msg).await;
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
