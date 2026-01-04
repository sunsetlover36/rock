use std::time::Duration;

use crate::{actor::gamemode::WorldEvent, runtime::actor::Actor};
use color_eyre::eyre::Result;
use futures_util::StreamExt;
use shared::{IndexerEvent, RedisChannel};
use tokio::sync::mpsc;

pub struct IndexerActor {
    pub world_event_tx: mpsc::Sender<WorldEvent>,
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
                    let _ = self.world_event_tx.send(WorldEvent::Indexer(event)).await;
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
