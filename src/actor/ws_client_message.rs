use shared::ClientMessage;
use tokio::sync::mpsc;

use crate::{actor::gamemode::GameModeCallback, actor::types::Actor};

pub struct ClientMessageActor {
    rx: mpsc::Receiver<ClientMessage>,
    world_event_tx: mpsc::Sender<GameModeCallback>,
}

#[async_trait::async_trait]
impl Actor for ClientMessageActor {
    async fn run(mut self: Box<Self>) {
        while let Some(msg) = self.rx.recv().await {
            let _ = self
                .world_event_tx
                .send(GameModeCallback::Client(msg))
                .await;
        }
    }
}

pub fn create_client_message_actor(
    buffer: usize,
    world_event_tx: mpsc::Sender<GameModeCallback>,
) -> (mpsc::Sender<ClientMessage>, ClientMessageActor) {
    let (tx, rx) = mpsc::channel::<ClientMessage>(buffer);

    let actor = ClientMessageActor { rx, world_event_tx };
    return (tx, actor);
}
