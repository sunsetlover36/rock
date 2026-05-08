use axum::{
    Json, Router,
    body::Bytes,
    extract::{ConnectInfo, Query, State, WebSocketUpgrade},
    http::{HeaderMap, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, post},
};
use color_eyre::eyre;
use hmac::{Hmac, KeyInit, Mac};
use rock_wire::{ImpromptuRequest, SocketConnectionQuery, farcaster::WebhookPayload};
use sha2::Sha512;
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

use crate::{
    config::{AuthKind, Config},
    runtime::{RuntimeCallback, SystemCallback},
    socket::{
        adapter::{SocketAdapter, SocketAdapterParams},
        auth::{AuthError, FarcasterVerifier, verify_auth},
        session_registry::SessionRegistrar,
    },
};

type HmacSha512 = Hmac<Sha512>;

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
    config: Arc<Config>,
    fc_verifier: Option<Arc<FarcasterVerifier>>,
}

pub struct ApiParams {
    pub session_registrar: SessionRegistrar,
    pub runtime_callback_tx: flume::Sender<RuntimeCallback>,
    pub config: Config,
    pub fc_verifier: Option<FarcasterVerifier>,
}
pub struct Api {
    app: Router,
}
impl Api {
    pub fn new(params: ApiParams) -> Self {
        let state = AppState {
            session_registrar: params.session_registrar,
            runtime_callback_tx: params.runtime_callback_tx.clone(),
            config: Arc::new(params.config),
            fc_verifier: params.fc_verifier.map(Arc::new),
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
        Query(mut query): Query<SocketConnectionQuery>,
    ) -> Response {
        let auth = match query
            .remove("auth")
            .and_then(|v| v.as_str().map(str::to_owned))
            .map(|v| v.parse::<AuthKind>())
            .transpose()
        {
            Ok(auth) => auth,
            Err(_) => return StatusCode::UNAUTHORIZED.into_response(),
        };

        let token = query
            .remove("token")
            .and_then(|v| v.as_str().map(str::to_owned));

        let auth_config = state
            .config
            .auth
            .as_ref()
            .filter(|c| !c.providers.is_empty());
        let identity = match (auth_config, auth, token.as_deref()) {
            (None, _, _) => None,
            (Some(auth_config), Some(auth), Some(token)) => {
                match verify_auth(auth_config, state.fc_verifier.as_deref(), auth, token) {
                    Ok(sub) => Some(sub),
                    Err(AuthError::Disabled) => None,
                    Err(_) => return StatusCode::UNAUTHORIZED.into_response(),
                }
            }
            (Some(_), _, _) => {
                return StatusCode::UNAUTHORIZED.into_response();
            }
        };

        ws.on_upgrade(async move |socket| {
            if let Err(err) = SocketAdapter::new(SocketAdapterParams {
                socket,
                session: state.session_registrar.register(identity),
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
        headers: HeaderMap,
        State(state): State<AppState>,
        body: Bytes,
    ) -> Result<(), StatusCode> {
        let sig = headers
            .get("X-Neynar-Signature")
            .ok_or(StatusCode::UNAUTHORIZED)?
            .to_str()
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
        let sig_bytes = hex::decode(sig).map_err(|_| StatusCode::UNAUTHORIZED)?;

        let webhook_env = state
            .config
            .farcaster
            .as_ref()
            .and_then(|f| f.webhook_env.as_ref())
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let webhook_secret =
            std::env::var(webhook_env).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let mut mac = HmacSha512::new_from_slice(webhook_secret.as_bytes())
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
        mac.update(&body);
        mac.verify_slice(&sig_bytes)
            .map_err(|_| StatusCode::UNAUTHORIZED)?;

        let payload: WebhookPayload =
            serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;
        state
            .runtime_callback_tx
            .send_async(RuntimeCallback::System(SystemCallback::Webhook(Box::new(
                payload.event,
            ))))
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        Ok(())
    }

    pub async fn listen(self, host: Option<String>, port: Option<u16>) -> eyre::Result<()> {
        let listener = TcpListener::bind(format!(
            "{}:{}",
            host.unwrap_or("127.0.0.1".to_string()),
            port.unwrap_or(3000)
        ))
        .await
        .unwrap();
        axum::serve(listener, self.app).await?;

        Ok(())
    }
}
