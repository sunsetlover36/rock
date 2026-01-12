use shared::OutgoingPacket;
use tokio::sync::{broadcast, mpsc};

use crate::actor::types::Actor;

pub struct ServerMessageActor {
    broadcast_rx: mpsc::Receiver<OutgoingPacket>,
    broadcaster: broadcast::Sender<OutgoingPacket>,
}

#[async_trait::async_trait]
impl Actor for ServerMessageActor {
    async fn run(mut self: Box<Self>) {
        while let Some(msg) = self.broadcast_rx.recv().await {
            println!("[ws server] socket sent: {:?}", msg);
            let _ = self.broadcaster.send(msg);
        }
    }
}

#[derive(Clone)]
pub struct ServerMessageHandle {
    broadcaster: broadcast::Sender<OutgoingPacket>,
}
impl ServerMessageHandle {
    pub fn subscribe(&self) -> broadcast::Receiver<OutgoingPacket> {
        self.broadcaster.subscribe()
    }
}

pub fn create_server_message_actor(
    buffer: usize,
) -> (
    ServerMessageHandle,
    ServerMessageActor,
    mpsc::Sender<OutgoingPacket>,
) {
    let (broadcaster, _) = broadcast::channel::<OutgoingPacket>(buffer);
    let (broadcast_tx, broadcast_rx) = mpsc::channel::<OutgoingPacket>(buffer);

    let handle = ServerMessageHandle {
        broadcaster: broadcaster.clone(),
    };
    let actor = ServerMessageActor {
        broadcaster,
        broadcast_rx,
    };

    (handle, actor, broadcast_tx)
}
