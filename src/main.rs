use std::{thread, time::Duration};

use axum::{
    Router,
    extract::{
        State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    response::Response,
    routing::{any, get},
};
use color_eyre::eyre::{self, Result};
use shared::IncomingRequest;
use tokio::{net::TcpListener, sync::mpsc};

use crate::{
    actor::{ActorRuntime, ws_client_message::create_client_message_actor},
    config::ServerConfig,
    envelope::ClientEnvelope,
    gamemode::{
        GameMode, GameModeCallback, GameModeParams,
        default_event_listener::GameModeDefaultEventListener,
    },
    meta_db::{MetaDb, MetaDbConfig},
    player_pool::PlayerPool,
    router::CommitRouter,
    socket::{
        adapter::SocketAdapter,
        session_registry::{SessionRegistrar, SessionRegistry},
    },
};

mod actor;
mod config;
mod envelope;
mod gamemode;
mod meta_db;
mod player_pool;
mod router;
mod socket;
mod utils;
mod world;

#[derive(Clone)]
struct AppState {
    session_registrar: SessionRegistrar,
    client_messenger_tx: mpsc::Sender<ClientEnvelope<IncomingRequest>>,
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}
async fn handle_socket(socket: WebSocket, state: AppState) {
    SocketAdapter::new(
        socket,
        state.session_registrar.register(),
        state.client_messenger_tx.clone(),
    )
    .activate()
    .await;
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv()?;

    let config_path = std::env::args()
        .nth(1)
        .ok_or_else(|| eyre::eyre!("Config path not set"))?;
    let config = ServerConfig::new(&config_path)?;

    let (gamemode_callback_tx, gamemode_callback_rx) = flume::bounded::<GameModeCallback>(1024);
    let (client_messenger_tx, client_messenger_actor) =
        create_client_message_actor(1024, gamemode_callback_tx.clone());

    // Actor Runtime for background async tasks
    // Actor #1: Route ws client messages to gamemode callbacks channel
    // More reasons to keep it? If no, get rid of it
    ActorRuntime::new().with(client_messenger_actor).start();

    // WS Session registry
    let session_registry = SessionRegistry::new(1024, 64, 8, PlayerPool::new());
    let session_registrar = session_registry.registrar();
    let session_sender = session_registry.sender();

    // Commit Router -> listen for and distribute new world events as they're committed
    let commit_router = CommitRouter::new(session_sender.clone());

    // Meta database
    let meta_db = MetaDb::new(MetaDbConfig {
        mode_id: config.gamemode_name.clone(),
        default_ttl: Duration::from_secs(30),
    })
    .await?;

    // GameMode main process
    thread::spawn(move || {
        let gm = GameMode::new(GameModeParams {
            name: config.gamemode_name,
            event_listener: Box::new(GameModeDefaultEventListener {
                ws_session_sender: session_sender.clone(),
            }),
            callback_rx: gamemode_callback_rx,
            commit_router,
            meta_db,
        })
        .unwrap();
        gm.awaken().unwrap();
    });

    // Axum HTTP/WS API
    let state = AppState {
        session_registrar,
        client_messenger_tx,
    };
    let app = Router::new()
        .route("/", get(async || "Hello, World!"))
        .route("/ws", any(ws_handler))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("Server is listening on 127.0.0.1:3000!");
    axum::serve(listener, app).await.unwrap();

    Ok(())
}
