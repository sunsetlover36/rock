pub mod indexer;
pub mod ws_client_message;

#[async_trait::async_trait]
pub trait Actor: Send + 'static {
    async fn run(self: Box<Self>);
}

pub struct ActorRuntime {
    actors: Vec<Box<dyn Actor>>,
}
impl ActorRuntime {
    pub fn new() -> Self {
        Self { actors: Vec::new() }
    }

    pub fn with<A: Actor>(mut self, actor: A) -> Self {
        self.actors.push(Box::new(actor));
        self
    }

    pub fn start(self) {
        for actor in self.actors {
            tokio::spawn(actor.run());
        }
    }
}
