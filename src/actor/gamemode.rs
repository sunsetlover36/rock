use shared::GameModeRequest;
use tokio::sync::mpsc;

use crate::actor::{
    Actor,
    world::{GameIntent, WorldGetters},
};

pub mod default_event_listener;
pub mod protocol;
pub use protocol::*;

pub trait GameModeEventListener: Send + Sync {
    fn on_emit(&self, event: GameModeEvent);
}

pub struct GameMode {
    pub gamemode_event_listener: Box<dyn GameModeEventListener>,
    pub gamemode_callback_rx: mpsc::Receiver<GameModeCallback>,
    pub game_intent_tx: mpsc::Sender<GameIntent>,
    pub world_getters: WorldGetters,
}
impl GameMode {
    // Untrusted input (called by the client)
    fn on_client_request(&self, message: ClientRequest) {
        println!("[gamemode] new client message: {:?}", message);
        self.gamemode_event_listener
            .on_emit(GameModeEvent::SendClientMessage {
                pk: message.sender,
                text: String::from("Hello from Wonderful RP!"),
            });

        match message.payload {
            GameModeRequest::PlayerMove(dir) => {
                // TODO: Who's being moved?
                self.game_intent_tx.send(GameIntent::MovePlayer(dir));
            }
        }
    }

    // Trusted input (called by the engine)
    fn on_engine_callback(&self, cb: EngineCallback) {
        match cb {
            EngineCallback::OnGameModeInit => {
                println!("[gamemode] gamemode init");
                // Load the world, initialize entities
            }
            EngineCallback::OnPlayerConnect { pk } => {
                println!("[gamemode] player connected: {:?}", pk);
                // Spawn player, include the player into the world
            }
        }
    }
}

#[async_trait::async_trait]
impl Actor for GameMode {
    async fn run(mut self: Box<Self>) {
        while let Some(msg) = self.gamemode_callback_rx.recv().await {
            match msg {
                GameModeCallback::Engine(cb) => {
                    self.on_engine_callback(cb);
                }
                GameModeCallback::Client(message) => {
                    self.on_client_request(message);
                }
                _ => {}
            }
        }
    }
}
