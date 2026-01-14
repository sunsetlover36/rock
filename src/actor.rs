pub mod gamemode;
pub mod indexer;
pub mod world;
pub mod ws_client_message;

#[async_trait::async_trait]
pub trait Actor: Send + 'static {
    async fn run(self: Box<Self>);
}
