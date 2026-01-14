use std::time::Duration;

use crate::actor::{Actor, gamemode::GameModeCallback};
use color_eyre::eyre::Result;
use futures_util::StreamExt;
use tokio::sync::mpsc;

pub mod protocol;
pub use protocol::*;

pub struct IndexerActor {
    pub gamemode_callback_tx: mpsc::Sender<GameModeCallback>,
    pub redis_url: String,
}

impl IndexerActor {
    async fn run_once(&mut self) -> Result<()> {
        let client = redis::Client::open(self.redis_url.as_str())?;
        let pubsub = client.get_async_pubsub().await?;
        let (mut sink, mut stream) = pubsub.split();

        sink.subscribe(RedisChannel::Tile.as_str()).await?;

        while let Some(msg) = stream.next().await {
            let payload: String = msg.get_payload()?;
            let json = serde_json::from_str::<IndexerEvent>(&payload);
            match json {
                Ok(event) => {
                    let _ = self
                        .gamemode_callback_tx
                        .send(GameModeCallback::Indexer(event))
                        .await;
                }
                Err(err) => {
                    eprintln!("[indexer] event listener error: {}", err);
                }
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl Actor for IndexerActor {
    async fn run(mut self: Box<Self>) {
        loop {
            if let Err(err) = self.run_once().await {
                eprintln!("redis actor crashed: {}, retrying", err);
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    }
}
