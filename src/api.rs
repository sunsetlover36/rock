use axum::{
    Json, Router,
    extract::{State, WebSocketUpgrade},
    http::StatusCode,
    response::Response,
    routing::{any, get, post},
};
use color_eyre::eyre;
use shared::ImpromptuRequest;
use tokio::net::TcpListener;

use crate::{
    runtime::{RuntimeCallback, SystemCallback},
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
            runtime_callback_tx: params.runtime_callback_tx.clone(),
        };

        let app = Router::new()
            .route("/", get(async || "Hello, World!"))
            .route("/ws", any(Api::handle_ws))
            .route("/impromptu", post(Api::process_impromptu))
            .with_state(state);

        Self { app }
    }

    async fn handle_ws(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
        ws.on_upgrade(async move |socket| {
            if let Err(err) = SocketAdapter::new(SocketAdapterParams {
                socket,
                session: state.session_registrar.register(),
                runtime_callback_tx: state.runtime_callback_tx.clone(),
            })
            .activate()
            .await
            {
                eprintln!("Failed to upgrade a socket: {}", err);
            };
        })
    }

    async fn process_impromptu(
        State(state): State<AppState>,
        Json(payload): Json<ImpromptuRequest>,
    ) -> Result<&'static str, StatusCode> {
        state
            .runtime_callback_tx
            .send_async(RuntimeCallback::System(SystemCallback::ImpromptuRequest {
                name: payload.name,
                code: payload.code,
            }))
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok("ok")
    }

    pub async fn listen(self) -> eyre::Result<()> {
        let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
        axum::serve(listener, self.app).await?;

        Ok(())
    }
}
