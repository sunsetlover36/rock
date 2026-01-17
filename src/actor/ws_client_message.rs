// This actor "unwraps" envelopes and routes them to needed receivers

use shared::IncomingRequest;
use tokio::sync::mpsc;

use crate::{
    actor::{Actor, gamemode::GameModeCallback},
    envelope::ClientEnvelope,
};

pub struct ClientMessageActor {
    rx: mpsc::Receiver<ClientEnvelope<IncomingRequest>>,
    gamemode_callback_tx: mpsc::Sender<GameModeCallback>,
}

#[async_trait::async_trait]
impl Actor for ClientMessageActor {
    async fn run(mut self: Box<Self>) {
        while let Some(msg) = self.rx.recv().await {
            match msg.payload {
                IncomingRequest::GameMode(req) => {
                    let _ = self
                        .gamemode_callback_tx
                        .send(GameModeCallback::Client(ClientEnvelope {
                            sender: msg.sender,
                            payload: req,
                        }))
                        .await;
                }
            }
        }
    }
}

pub fn create_client_message_actor(
    buffer: usize,
    gamemode_callback_tx: mpsc::Sender<GameModeCallback>,
) -> (
    mpsc::Sender<ClientEnvelope<IncomingRequest>>,
    ClientMessageActor,
) {
    let (tx, rx) = mpsc::channel::<ClientEnvelope<IncomingRequest>>(buffer);

    let actor = ClientMessageActor {
        rx,
        gamemode_callback_tx,
    };
    return (tx, actor);
}
