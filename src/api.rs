use axum::{
    Router,
    extract::{State, WebSocketUpgrade},
    response::Response,
    routing::{any, get},
};
use color_eyre::eyre;
use tokio::net::TcpListener;

use crate::{
    runtime::RuntimeCallback,
    socket::{
        adapter::{SocketAdapter, SocketAdapterParams},
        session_registry::SessionRegistrar,
    },
};

#[derive(Clone)]
struct AppState {
    session_registrar: SessionRegistrar,
    runtime_callback_tx: flume::Sender<RuntimeCallback>,
}

pub struct ApiParams {
    pub session_registrar: SessionRegistrar,
    pub runtime_callback_tx: flume::Sender<RuntimeCallback>,
}
pub struct Api {
    app: Router,
}
impl Api {
    pub fn new(params: ApiParams) -> Self {
        let state = AppState {
            session_registrar: params.session_registrar,
            runtime_callback_tx: params.runtime_callback_tx,
        };
        let app = Router::new()
            .route("/", get(async || "Hello, World!"))
            .route("/ws", any(Api::handle_ws))
            .with_state(state);

        Self { app }
    }

    async fn handle_ws(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
        ws.on_upgrade(async move |socket| {
            SocketAdapter::new(SocketAdapterParams {
                socket,
                session: state.session_registrar.register(),
                runtime_callback_tx: state.runtime_callback_tx.clone(),
            })
            .activate()
            .await;
        })
    }

    pub async fn listen(self) -> eyre::Result<()> {
        let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
        axum::serve(listener, self.app).await?;

        Ok(())
    }
}
