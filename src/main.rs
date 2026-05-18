use std::{fs, path::Path, sync::Arc, thread, time::Duration};

use clap::Parser;
use color_eyre::eyre;
use tokio::runtime::Handle;

use crate::{
    api::{Api, ApiParams},
    cli::Cli,
    clients::FarcasterApi,
    config::Config,
    meta_db::{MetaDb, MetaDbConfig},
    player_pool::PlayerPool,
    router::CommitRouter,
    runtime::{
        Runtime, RuntimeCallback, RuntimeCommand, RuntimeExit, RuntimeParams,
        default_client_api::GameModeDefaultClientApi,
    },
    socket::{
        auth::FarcasterVerifier,
        session_registry::{SessionRegistry, SessionRegistryParams},
    },
    watcher::spawn_reload_watcher,
};

mod api;
mod cli;
mod clients;
mod config;
mod envelope;
mod meta_db;
mod player_pool;
mod router;
mod runtime;
mod rx;
mod socket;
mod utils;
mod watcher;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err}");
        std::process::exit(1)
    }
}

async fn run() -> eyre::Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        cli::Command::Ignite => {
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

            // Runtime main process (single-threaded)
            let (runtime_callback_tx, runtime_callback_rx) =
                flume::bounded::<RuntimeCallback>(1024);
            let (runtime_cmd_tx, runtime_cmd_rx) = flume::bounded::<RuntimeCommand>(32);

            // Load config
            let config = Config::new()?;
            let runtime_config = config.clone();
            let api_config = config.clone();

            // Hot reload watcher
            let _watcher_thread =
                spawn_reload_watcher(runtime_config.gamemode.name.clone(), runtime_cmd_tx);

            thread::spawn(move || {
                fn should_reload_runtime(cmd_rx: &flume::Receiver<RuntimeCommand>) -> bool {
                    match cmd_rx.recv() {
                        Ok(RuntimeCommand::Reload) => {
                            println!("[HRM] Reloading a runtime...");
                            true
                        }
                        Ok(RuntimeCommand::Shutdown) | Err(_) => false,
                    }
                }

                loop {
                    // Meta database
                    let meta_db = match tokio_handle.block_on(MetaDb::new(MetaDbConfig {
                        mode_id: runtime_config.gamemode.name.clone(),
                        default_ttl: Duration::from_secs(30),
                    })) {
                        Ok(db) => db,
                        Err(err) => {
                            eprintln!("Failed to initialize MetaDb: {err}");
                            break;
                        }
                    };

                    let fc_api = if let Some(config) = runtime_config.farcaster.as_ref() {
                        if let Some(key) = &config.api_key {
                            match FarcasterApi::new(key, config.signers.clone()) {
                                Ok(api) => Some(api),
                                Err(err) => {
                                    eprintln!("Failed to initialize Farcaster API: {err}");
                                    break;
                                }
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let runtime_params = RuntimeParams {
                        config: runtime_config.clone(),
                        client_api: Arc::new(GameModeDefaultClientApi {
                            ws_session_sender: session_sender.clone(),
                        }),
                        callback_rx: runtime_callback_rx.clone(),
                        command_rx: runtime_cmd_rx.clone(),
                        commit_router: commit_router.clone(),
                        meta_db,
                        fc_api,
                        tokio_handle: tokio_handle.clone(),
                    };
                    let mut runtime = match Runtime::new(runtime_params) {
                        Ok(r) => r,
                        Err(err) => {
                            eprintln!("[HRM] Failed to boot up a new runtime: {err}");

                            if !should_reload_runtime(&runtime_cmd_rx) {
                                break;
                            }
                            continue;
                        }
                    };

                    match runtime.awaken() {
                        Ok(RuntimeExit::Reload) => {
                            println!("[HRM] Reloading a runtime...");
                        }
                        Ok(RuntimeExit::Shutdown) => {
                            break;
                        }
                        Err(err) => {
                            eprintln!("[HRM] Runtime crashed: {err}");

                            if !should_reload_runtime(&runtime_cmd_rx) {
                                break;
                            }
                        }
                    }
                }
            });

            // Axum API
            let fc_verifier =
                if let Some(c) = api_config.auth.as_ref().and_then(|c| c.farcaster.as_ref()) {
                    Some(FarcasterVerifier::new(c).await?)
                } else {
                    None
                };
            Api::new(ApiParams {
                session_registrar,
                runtime_callback_tx: runtime_callback_tx.clone(),
                config: api_config,
                fc_verifier,
            })
            .listen(
                std::env::var("HOST").ok(),
                std::env::var("PORT").ok().and_then(|p| p.parse().ok()),
            )
            .await?;
        }
        cli::Command::Genesis { name } => {
            fs::create_dir_all("./gamemodes")?;
            fs::create_dir_all("./assets")?;

            let sample_gamemode = r#"on.world.awake()
    :each(function ()
    print("Hello, World!")
    end)
    "#;
            fs::write(format!("./gamemodes/{}.lua", name.clone()), sample_gamemode)?;

            let config_path = Path::new(Config::filename());
            let mut config: Config = if config_path.exists() {
                let content = fs::read_to_string(config_path)?;
                toml::from_str(&content)?
            } else {
                Config::default()
            };

            config.gamemode.name = name.clone();
            let content = toml::to_string_pretty(&config)?;
            fs::write(config_path, content)?;

            println!(
                "Bootstrapped gamemodes/{}.lua! Use `rock ignite` to start the runtime.",
                name
            );
        } // cli::Command::Accrete { geode_name } => {}
          // cli::Command::Scan => {}
    }

    Ok(())
}
