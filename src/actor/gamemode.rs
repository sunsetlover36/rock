pub mod default_event_listener;
pub mod types;
use shared::ClientMessage;
pub use types::*;

use tokio::sync::mpsc;

use crate::{
    actor::types::Actor,
    world::{GameIntent, WorldGetters},
};

pub struct GameMode {
    pub gamemode_event_listener: Box<dyn GameModeEventListener>,
    pub gamemode_callback_rx: mpsc::Receiver<GameModeCallback>,
    pub game_intent_tx: mpsc::Sender<GameIntent>,
    pub world_getters: WorldGetters,
}
impl GameMode {
    fn on_client_message(&self, message: ClientMessage) {
        println!("[gamemode] new client message: {:?}", message);
        self.gamemode_event_listener
            .on_emit(GameModeEvent::SendClientMessage {
                pk: message.sender,
                text: String::from("Hello from Wonderful RP!"),
            });
    }
}

#[async_trait::async_trait]
impl Actor for GameMode {
    async fn run(mut self: Box<Self>) {
        while let Some(msg) = self.gamemode_callback_rx.recv().await {
            match msg {
                GameModeCallback::Client(message) => {
                    self.on_client_message(message);
                }
                _ => {}
            }
        }
    }
}
