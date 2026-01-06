use shared::IndexerEvent;
use tokio::sync::mpsc;

use crate::{
    actor::ws::{client_message::ClientMessage, server_message::ServerMessage},
    runtime::actor::Actor,
    world::{GameIntent, WorldGetters},
};

pub enum GameModeCallback {
    Client(ClientMessage),
    Indexer(IndexerEvent),
}

pub struct GameMode {
    // TODO: implement a Broadcaster trait with broadcast and send_private methods
    // ----  so GameMode can be channel-agnostic
    pub broadcaster: mpsc::Sender<ServerMessage>,
    pub gamemode_callback_rx: mpsc::Receiver<GameModeCallback>,
    pub game_intent_tx: mpsc::Sender<GameIntent>,
    pub world_getters: WorldGetters,
}
impl GameMode {
    fn handle_client_message(&self, client_msg: ClientMessage) {
        println!("[gamemode] new client message: {:?}", client_msg);
    }
}

#[async_trait::async_trait]
impl Actor for GameMode {
    async fn run(mut self: Box<Self>) {
        while let Some(msg) = self.gamemode_callback_rx.recv().await {
            match msg {
                GameModeCallback::Client(client_msg) => {
                    self.handle_client_message(client_msg);
                }
                _ => {}
            }
        }
    }
}
