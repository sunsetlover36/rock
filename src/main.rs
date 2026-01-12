use std::sync::Arc;

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
use dotenvy::dotenv;
use tokio::{net::TcpListener, sync::mpsc};

use crate::{
    actor::{
        gamemode::{GameMode, GameModeCallback},
        ws::{
            client_message::{ClientMessage, create_client_message_actor},
            server_message::{ServerMessageHandle, create_server_message_actor},
        },
    },
    router::CommitRouter,
    runtime::Runtime,
    socket::SocketAdapter,
    state::WorldState,
    world::create_world_actor,
};

mod actor;
mod player_pool;
mod router;
mod runtime;
mod socket;
mod state;
mod world;

#[derive(Clone)]
struct AppState {
    server_messenger_handle: Arc<ServerMessageHandle>,
    client_messenger_tx: mpsc::Sender<ClientMessage>,
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}
async fn handle_socket(socket: WebSocket, state: AppState) {
    let server_message_rx = state.server_messenger_handle.subscribe();

    SocketAdapter::new(socket, server_message_rx, state.client_messenger_tx)
        .activate()
        .await;
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let world_state = WorldState::new();

    let (gamemode_callback_tx, gamemode_callback_rx) = mpsc::channel::<GameModeCallback>(1024);

    let (server_messenger_handle, server_messenger_actor, broadcaster) =
        create_server_message_actor(1024);
    let server_messenger_handle = Arc::new(server_messenger_handle);

    let (client_messenger_tx, client_messenger_actor) =
        create_client_message_actor(1024, gamemode_callback_tx.clone());

    let commit_router = CommitRouter::new(broadcaster.clone());
    let (game_intent_tx, world_actor, world_getters) =
        create_world_actor(2048, commit_router, world_state);

    // Redis
    /*
    let redis_url = env::var("REDIS_URL").expect("REDIS_URL not set");
    let indexer_actor = IndexerActor {
        game_intent_tx: game_intent_tx.clone(),
        redis_url,
    };
    */

    let gamemode = GameMode {
        broadcaster,
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
        .with(server_messenger_actor)
        .with(client_messenger_actor)
        .with(gamemode)
        .start();

    // Process dictator: Axum HTTP/WS API
    let state = AppState {
        server_messenger_handle,
        client_messenger_tx: client_messenger_tx.clone(),
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
