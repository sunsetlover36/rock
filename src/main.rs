use axum::{
    Router,
    extract::{
        State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    response::Response,
    routing::{any, get},
};
use color_eyre::eyre::Result;
use shared::ClientMessage;
use tokio::{net::TcpListener, sync::mpsc};

use crate::{
    actor::{
        gamemode::{
            GameMode, GameModeCallback, default_event_listener::GameModeDefaultEventListener,
        },
        world::create_world_actor,
        ws_client_message::create_client_message_actor,
    },
    player_pool::PlayerPool,
    router::CommitRouter,
    runtime::Runtime,
    socket::{
        adapter::SocketAdapter,
        session_registry::{SessionRegistrar, SessionRegistry},
    },
};

mod actor;
mod meta_db;
mod player_pool;
mod router;
mod runtime;
mod socket;

#[derive(Clone)]
struct AppState {
    session_registrar: SessionRegistrar,
    client_messenger_tx: mpsc::Sender<ClientMessage>,
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

    let (gamemode_callback_tx, gamemode_callback_rx) = mpsc::channel::<GameModeCallback>(1024);

    let (client_messenger_tx, client_messenger_actor) =
        create_client_message_actor(1024, gamemode_callback_tx.clone());

    let session_registry = SessionRegistry::new(1024, 64, PlayerPool::new());
    let session_registrar = session_registry.registrar();
    let session_sender = session_registry.sender();

    let commit_router = CommitRouter::new(session_sender.clone());
    let (game_intent_tx, world_actor, world_getters) = create_world_actor(2048, commit_router);

    let gamemode = GameMode {
        gamemode_event_listener: Box::new(GameModeDefaultEventListener {
            ws_session_sender: session_sender.clone(),
        }),
        gamemode_callback_rx,
        game_intent_tx,
        world_getters,
    };

    // Runtime
    // 1. Set up world actor (global game intents listener)
    // 2. Set up ws server-to-clients broadcast channel
    // 3. Route ws client messages to gamemode callbacks channel
    // 4. Gamemode starts listening for orders (e.g., client messages)
    Runtime::new()
        .with(world_actor)
        .with(client_messenger_actor)
        .with(gamemode)
        .start();

    // Process dictator: Axum HTTP/WS API
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
