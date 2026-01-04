use serde::{Deserialize, Serialize};
use shared::Tile;
use tokio::sync::{broadcast, mpsc};

use crate::runtime::actor::Actor;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    TileExplored { tile: Tile },
    TileRemoved { tx_hash: String, x: i64, y: i64 },
    GameTick { timestamp: u64 },
}

pub struct ServerMessageActor {
    propagator_rx: mpsc::Receiver<ServerMessage>,
    broadcaster: broadcast::Sender<ServerMessage>,
}

#[async_trait::async_trait]
impl Actor for ServerMessageActor {
    async fn run(mut self: Box<Self>) {
        while let Some(msg) = self.propagator_rx.recv().await {
            println!("[broadcast] socket sent: {:?}", msg);
            let _ = self.broadcaster.send(msg);
        }
    }
}

#[derive(Clone)]
pub struct ServerMessageHandle {
    broadcaster: broadcast::Sender<ServerMessage>,
}
impl ServerMessageHandle {
    pub fn subscribe(&self) -> broadcast::Receiver<ServerMessage> {
        self.broadcaster.subscribe()
    }
}

pub fn create_server_message_actor(
    buffer: usize,
) -> (
    ServerMessageHandle,
    ServerMessageActor,
    mpsc::Sender<ServerMessage>,
) {
    let (broadcaster, _) = broadcast::channel::<ServerMessage>(buffer);
    let (propagator_tx, propagator_rx) = mpsc::channel::<ServerMessage>(buffer);

    let handle = ServerMessageHandle {
        broadcaster: broadcaster.clone(),
    };
    let actor = ServerMessageActor {
        broadcaster,
        propagator_rx,
    };

    (handle, actor, propagator_tx)
}
