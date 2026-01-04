use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{actor::gamemode::WorldEvent, runtime::actor::Actor};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Move { x: i64, y: i64 },
}

pub struct ClientMessageActor {
    rx: mpsc::Receiver<ClientMessage>,
    world_event_tx: mpsc::Sender<WorldEvent>,
}

#[async_trait::async_trait]
impl Actor for ClientMessageActor {
    async fn run(mut self: Box<Self>) {
        while let Some(msg) = self.rx.recv().await {
            let _ = self.world_event_tx.send(WorldEvent::Client(msg)).await;
        }
    }
}

pub fn create_client_message_actor(
    buffer: usize,
    world_event_tx: mpsc::Sender<WorldEvent>,
) -> (mpsc::Sender<ClientMessage>, ClientMessageActor) {
    let (tx, rx) = mpsc::channel::<ClientMessage>(buffer);

    let actor = ClientMessageActor { rx, world_event_tx };
    return (tx, actor);
}
