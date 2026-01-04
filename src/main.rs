use std::{env, sync::Arc};

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
        gamemode::{GameMode, WorldEvent},
        indexer::IndexerActor,
        ws::{
            client_message::{ClientMessage, create_client_message_actor},
            server_message::{ServerMessageHandle, create_server_message_actor},
        },
    },
    runtime::Runtime,
    socket::adapter::SocketAdapter,
};

mod actor;
mod runtime;
mod socket;

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

    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();

    let (world_event_tx, world_event_rx) = mpsc::channel::<WorldEvent>(1024);

    let (server_messenger_handle, server_messenger_actor, propagator) =
        create_server_message_actor(1024);
    let server_messenger_handle = Arc::new(server_messenger_handle);

    let (client_messenger_tx, client_messenger_actor) =
        create_client_message_actor(1024, world_event_tx.clone());

    // Redis
    /*
    let redis_url = env::var("REDIS_URL").expect("REDIS_URL not set");
    let indexer_actor = IndexerActor {
        world_event_tx: world_event_tx.clone(),
        redis_url,
    };
    */
    let gamemode = GameMode {
        propagator,
        world_event_rx,
    };

    // Runtime
    // 1. Listen for client messages
    // 2. Set up broadcaster
    // 3. Listen for world events
    Runtime::new()
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

    println!("Server is listening on 127.0.0.1:3000!");
    axum::serve(listener, app).await.unwrap();

    Ok(())
}
