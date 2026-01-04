use shared::IndexerEvent;
use tokio::sync::mpsc;

use crate::{
    actor::ws::{client_message::ClientMessage, server_message::ServerMessage},
    runtime::actor::Actor,
};

pub enum WorldEvent {
    Client(ClientMessage),
    Indexer(IndexerEvent),
    Tick { ts: u64 },
}

pub struct GameMode {
    pub world_event_rx: mpsc::Receiver<WorldEvent>,
    pub propagator: mpsc::Sender<ServerMessage>,
}
impl GameMode {
    fn handle_client_message(&self, client_msg: ClientMessage) {
        println!("[gamemode] new client message: {:?}", client_msg);
        let _ = self
            .propagator
            .send(ServerMessage::GameTick { timestamp: 1 });
    }
    fn handle_indexer_event(&self, indexer_evt: IndexerEvent) {
        println!("[gamemode] new indexer event: {:?}", indexer_evt);
    }
}

#[async_trait::async_trait]
impl Actor for GameMode {
    async fn run(mut self: Box<Self>) {
        while let Some(msg) = self.world_event_rx.recv().await {
            match msg {
                WorldEvent::Client(client_msg) => {
                    self.handle_client_message(client_msg);
                }
                WorldEvent::Indexer(indexer_evt) => {
                    self.handle_indexer_event(indexer_evt);
                }
                WorldEvent::Tick { ts } => {
                    println!("[gamemode] tick called: ts = {}", ts);
                }
            }
        }
    }
}
