use axum::{
    Json, Router,
    extract::{ConnectInfo, Query, State, WebSocketUpgrade},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{any, post},
};
use color_eyre::eyre;
use shared::{ImpromptuRequest, SocketConnectionQuery, farcaster::WebhookPayload};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

use crate::{
    runtime::{RuntimeCallback, SystemCallback},
    socket::{
        adapter::{SocketAdapter, SocketAdapterParams},
        session_registry::SessionRegistrar,
    },
};

async fn localhost_only(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if !addr.ip().is_loopback() {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(next.run(req).await)
}

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
            .route("/", any(Api::handle_ws))
            .route(
                "/impromptu",
                post(Api::process_impromptu).route_layer(middleware::from_fn(localhost_only)),
            )
            .route("/farcaster-webhook", post(Api::process_webhook))
            .nest_service("/assets", ServeDir::new("./assets"))
            .with_state(state);

        Self { app }
    }

    async fn handle_ws(
        ws: WebSocketUpgrade,
        State(state): State<AppState>,
        Query(query): Query<SocketConnectionQuery>,
    ) -> Response {
        ws.on_upgrade(async move |socket| {
            if let Err(err) = SocketAdapter::new(SocketAdapterParams {
                socket,
                session: state.session_registrar.register(),
                runtime_callback_tx: state.runtime_callback_tx.clone(),
                query,
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

    async fn process_webhook(
        State(state): State<AppState>,
        Json(payload): Json<WebhookPayload>,
    ) -> Result<(), StatusCode> {
        state
            .runtime_callback_tx
            .send_async(RuntimeCallback::System(SystemCallback::Webhook(Box::new(
                payload.event,
            ))))
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        Ok(())
    }

    pub async fn listen(self, port: Option<u16>) -> eyre::Result<()> {
        let listener = TcpListener::bind(format!("localhost:{}", port.unwrap_or(3000)))
            .await
            .unwrap();
        axum::serve(listener, self.app).await?;

        Ok(())
    }
}
