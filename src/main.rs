use std::{thread, time::Duration};

use clap::Parser;
use color_eyre::eyre::Result;
use tokio::runtime::Handle;

use crate::{
    api::{Api, ApiParams},
    cli::Cli,
    config::ServerConfig,
    meta_db::{MetaDb, MetaDbConfig},
    player_pool::PlayerPool,
    router::CommitRouter,
    runtime::{
        Runtime, RuntimeCallback, RuntimeParams, default_client_api::GameModeDefaultClientApi,
    },
    socket::session_registry::{SessionRegistry, SessionRegistryParams},
};

mod api;
mod cli;
mod config;
mod envelope;
mod meta_db;
mod player_pool;
mod router;
mod runtime;
mod rx;
mod socket;
mod utils;
mod world;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        cli::Command::Ignite { config } => {
            let config = ServerConfig::new(&config)?;

            let tokio_handle = Handle::current();

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

            // Runtime main process (single-threaded)
            let (runtime_callback_tx, runtime_callback_rx) =
                flume::bounded::<RuntimeCallback>(1024);
            let runtime_params = RuntimeParams {
                name: config.gamemode_name,
                client_api: Box::new(GameModeDefaultClientApi {
                    ws_session_sender: session_sender.clone(),
                }),
                callback_rx: runtime_callback_rx,
                commit_router,
                meta_db,
                tokio_handle,
            };
            thread::spawn(move || {
                let mut runtime = Runtime::new(runtime_params).unwrap();
                runtime.awaken().unwrap();
            });

            // Axum API
            Api::new(ApiParams {
                session_registrar,
                runtime_callback_tx: runtime_callback_tx.clone(),
            })
            .listen()
            .await?;
        }
        cli::Command::Genesis { name } => {}
        cli::Command::Accrete { geode_name } => {}
        cli::Command::Scan => {}
    }

    Ok(())
}
