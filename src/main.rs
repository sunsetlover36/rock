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
use tokio::{net::TcpListener, runtime::Handle, sync::mpsc};

use crate::{
    actor::{ActorRuntime, client_message::create_client_message_actor},
    config::ServerConfig,
    envelope::ClientEnvelope,
    meta_db::{MetaDb, MetaDbConfig},
    player_pool::PlayerPool,
    router::CommitRouter,
    runtime::{
        Runtime, RuntimeCallback, RuntimeParams, default_client_api::GameModeDefaultClientApi,
    },
    socket::{
        adapter::{SocketAdapter, SocketAdapterParams},
        session_registry::{SessionRegistrar, SessionRegistry, SessionRegistryParams},
    },
};

mod actor;
mod config;
mod envelope;
mod meta_db;
mod player_pool;
mod router;
mod runtime;
mod socket;
mod utils;
mod world;

#[derive(Clone)]
struct AppState {
    session_registrar: SessionRegistrar,
    client_messenger_tx: mpsc::Sender<ClientEnvelope<IncomingRequest>>,
    gamemode_callback_tx: flume::Sender<RuntimeCallback>,
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}
async fn handle_socket(socket: WebSocket, state: AppState) {
    SocketAdapter::new(SocketAdapterParams {
        socket,
        session: state.session_registrar.register(),
        client_messenger_tx: state.client_messenger_tx.clone(),
        gamemode_callback_tx: state.gamemode_callback_tx.clone(),
    })
    .activate()
    .await;
}

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = std::env::args()
        .nth(1)
        .ok_or_else(|| eyre::eyre!("Config path not set"))?;
    let config = ServerConfig::new(&config_path)?;

    let tokio_handle = Handle::current();

    let (gamemode_callback_tx, gamemode_callback_rx) = flume::bounded::<RuntimeCallback>(1024);
    let (client_messenger_tx, client_messenger_actor) =
        create_client_message_actor(1024, gamemode_callback_tx.clone());

    // TODO: Client messenger actor acts like an unwrapper for incoming requests. Remove it
    // Actor Runtime for background async tasks
    // Actor #1: Route ws client messages to gamemode callbacks channel
    // More reasons to keep it? If no, get rid of it
    ActorRuntime::new().with(client_messenger_actor).start();

    // WS Session registry
    let session_registry = SessionRegistry::new(SessionRegistryParams {
        broadcast_hub_buffer: 1024,
        session_channel_buffer: 256,
        player_pool: PlayerPool::new(),
        tokio_handle: tokio_handle.clone(),
    });
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

    // Runtime main process
    let runtime_params = RuntimeParams {
        name: config.gamemode_name,
        client_api: Box::new(GameModeDefaultClientApi {
            ws_session_sender: session_sender.clone(),
        }),
        callback_rx: gamemode_callback_rx,
        commit_router,
        meta_db,
        tokio_handle,
    };
    thread::spawn(move || {
        let mut runtime = Runtime::new(runtime_params).unwrap();
        runtime.awaken().unwrap();
    });

    // Axum HTTP/WS API
    let state = AppState {
        session_registrar,
        client_messenger_tx,
        gamemode_callback_tx,
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
