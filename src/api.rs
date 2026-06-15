use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode, header::ORIGIN},
    response::{IntoResponse, Response},
    routing::{any, post},
};
use color_eyre::eyre;
use hmac::{Hmac, KeyInit, Mac};
use rock_wire::{ImpromptuRequest, SocketConnectionQuery, farcaster::WebhookPayload};
use sha2::Sha512;
use std::sync::Arc;
use subtle::ConstantTimeEq;
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

const IMPROMPTU_TOKEN_ENV: &str = "ROCK_IMPROMPTU_TOKEN";
const IMPROMPTU_TOKEN_HEADER: &str = "X-Rock-Impromptu-Token";
const ALLOWED_ORIGINS_ENV: &str = "ROCK_ALLOWED_ORIGINS";
const IMPROMPTU_BODY_LIMIT: usize = 256 * 1024;
const WEBHOOK_BODY_LIMIT: usize = 1024 * 1024;

fn get_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;

    cookie.split(';').find_map(|part| {
        let part = part.trim();
        let (key, value) = part.split_once('=')?;

        if key == name {
            Some(value.to_string())
        } else {
            None
        }
    })
}

fn verify_impromptu_token(headers: &HeaderMap) -> Result<(), StatusCode> {
    let expected = std::env::var(IMPROMPTU_TOKEN_ENV)
        .ok()
        .filter(|token| !token.is_empty())
        .ok_or(StatusCode::NOT_FOUND)?;
    let actual = headers
        .get(IMPROMPTU_TOKEN_HEADER)
        .and_then(|token| token.to_str().ok())
        .ok_or(StatusCode::NOT_FOUND)?;

    if actual.as_bytes().ct_eq(expected.as_bytes()).into() {
        Ok(())
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

fn normalize_origin(origin: &str) -> &str {
    origin.trim().trim_end_matches('/')
}

fn parse_allowed_origins() -> Vec<String> {
    std::env::var(ALLOWED_ORIGINS_ENV)
        .unwrap_or_default()
        .split(',')
        .filter_map(|origin| {
            let origin = normalize_origin(origin);
            if origin.is_empty() {
                None
            } else {
                Some(origin.to_owned())
            }
        })
        .collect()
}

fn verify_ws_origin(headers: &HeaderMap, allowed_origins: &[String]) -> Result<(), StatusCode> {
    if allowed_origins.is_empty() {
        return Err(StatusCode::FORBIDDEN);
    }

    let origin = headers
        .get(ORIGIN)
        .and_then(|origin| origin.to_str().ok())
        .map(normalize_origin)
        .ok_or(StatusCode::FORBIDDEN)?;

    if allowed_origins.iter().any(|allowed| allowed == origin) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

#[derive(Clone)]
struct AppState {
    session_registrar: SessionRegistrar,
    runtime_callback_tx: flume::Sender<RuntimeCallback>,
    config: Arc<Config>,
    fc_verifier: Option<Arc<FarcasterVerifier>>,
    allowed_origins: Arc<Vec<String>>,
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
            allowed_origins: Arc::new(parse_allowed_origins()),
        };

        let app = Router::new()
            .route("/", any(Api::handle_ws))
            .route(
                "/impromptu",
                post(Api::process_impromptu).layer(DefaultBodyLimit::max(IMPROMPTU_BODY_LIMIT)),
            )
            .route(
                "/farcaster-webhook",
                post(Api::process_webhook).layer(DefaultBodyLimit::max(WEBHOOK_BODY_LIMIT)),
            )
            .nest_service("/assets", ServeDir::new("./assets"))
            .with_state(state);
        Self { app }
    }

    async fn handle_ws(
        ws: WebSocketUpgrade,
        State(state): State<AppState>,
        headers: HeaderMap,
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
        let explicit_auth = auth.is_some();
        let cookie_name =
            std::env::var("ROCK_SESSION_COOKIE").unwrap_or_else(|_| "rock_session".to_string());

        let query_token = query
            .remove("token")
            .and_then(|v| v.as_str().map(str::to_owned));
        let cookie_token = get_cookie(&headers, &cookie_name);
        let token = query_token.as_deref().or(cookie_token.as_deref());
        let token_from_cookie = query_token.is_none() && cookie_token.is_some();
        let auth_config = state
            .config
            .auth
            .as_ref()
            .filter(|c| !c.providers.is_empty());
        if auth_config.is_some()
            && let Err(status) = verify_ws_origin(&headers, &state.allowed_origins)
        {
            return status.into_response();
        }

        let auth = match (auth, auth_config) {
            (Some(auth), _) => Some(auth),
            (None, Some(auth_config)) if auth_config.providers.len() == 1 => {
                auth_config.providers.first().copied()
            }
            (None, Some(_)) => return StatusCode::UNAUTHORIZED.into_response(),
            (None, None) => None,
        };

        let identity = match (auth_config, auth, token) {
            (None, _, _) => None,
            (Some(auth_config), Some(auth), Some(token)) => {
                match verify_auth(auth_config, state.fc_verifier.as_deref(), auth, token) {
                    Ok(sub) => Some(sub),
                    Err(AuthError::Disabled) => None,
                    Err(_)
                        if auth_config.allow_anonymous && !explicit_auth && token_from_cookie =>
                    {
                        None
                    }
                    Err(_) => {
                        return StatusCode::UNAUTHORIZED.into_response();
                    }
                }
            }
            (Some(auth_config), _, None) if auth_config.allow_anonymous => None,
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
        headers: HeaderMap,
        State(state): State<AppState>,
        Json(payload): Json<ImpromptuRequest>,
    ) -> Result<&'static str, StatusCode> {
        verify_impromptu_token(&headers)?;

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
        let host = host.unwrap_or_else(|| "127.0.0.1".to_string());
        let port = port.unwrap_or(3000);
        let addr = format!("{host}:{port}");

        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|err| eyre::eyre!("Failed to bind {addr}: {err}"))?;

        axum::serve(listener, self.app)
            .await
            .map_err(|err| eyre::eyre!("Server error: {err}"))?;

        Ok(())
    }
}
