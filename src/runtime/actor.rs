#[async_trait::async_trait]
pub trait Actor: Send + 'static {
    async fn run(self: Box<Self>);
}
