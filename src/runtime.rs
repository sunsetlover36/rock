use std::{
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use color_eyre::eyre;
use mlua::Lua;
use rock_wire::{ImpromptuRequest, IncomingRequest};
use smallvec::smallvec;

use crate::{
    clients::FarcasterApi, config::Config, crypto::Crypto, meta_db::MetaDb, router::CommitRouter,
};

pub mod default_client_api;
pub(crate) mod event_bus;
pub(crate) use event_bus::EventBus;

pub(crate) mod plugins;
use plugins::{
    ConstantsPlugin, EntityPlugin, FarcasterPlugin, InputPlugin, JsonPlugin, LayerPlugin,
    MemoryPlugin, OnPlugin, PlayerPlugin, PluginComposer, RockPlugin, RoomPlugin, ScenePlugin,
    TimerPlugin,
    on::{
        event_descriptors::GLOBAL_EVENT_DESCRIPTORS,
        protocol::{
            EventScope, FarcasterEventData, GameModeEvent, GameModeEventData, PlayerEventData,
            WorldEventData,
        },
    },
    player::PlayerHandle,
    protocol::GameModePlugin,
    scene::{SceneManager, SceneManagerMessage, SceneManagerParams},
};

pub(crate) mod app_data;
use app_data::{BlueprintRegistry, ExecutionContext, InputEventRegistry, LayerRegistry};

pub(crate) mod network_replicator;
use network_replicator::{FieldRegistry, NetworkReplicator};

mod geode;
mod script;

pub(crate) mod protocol;
pub use protocol::*;

mod timer_manager;
use timer_manager::{TimerManager, TimerManagerParams};

mod utils;
pub use utils::*;

#[derive(Clone)]
pub struct RuntimeParams {
    pub config: Config,
    pub client_api: Arc<dyn GameModeClientApi>,
    pub callback_rx: flume::Receiver<RuntimeCallback>,
    pub command_rx: flume::Receiver<RuntimeCommand>,
    pub commit_router: CommitRouter,
    pub meta_db: MetaDb,
    pub crypto: Option<Crypto>,
    pub fc_api: Option<FarcasterApi>,
    pub tokio_handle: tokio::runtime::Handle,
}

pub struct Runtime {
    tick: u64,
    lua: Lua,
    callback_rx: flume::Receiver<RuntimeCallback>,
    command_rx: flume::Receiver<RuntimeCommand>,
    commit_router: CommitRouter,
    scene_manager: SceneManager,
    event_bus: Rc<EventBus>,
    timer_manager: Rc<TimerManager>,
    replicator: Rc<NetworkReplicator>,
}
impl Runtime {
    pub fn new(params: RuntimeParams) -> eyre::Result<Self> {
        let lua = Lua::new();
        let client_api = params.client_api.clone();

        // Dependencies
        let meta_db = Arc::new(params.meta_db);
        let event_bus = Rc::new(EventBus::new());
        let timer_manager = Rc::new(TimerManager::new(TimerManagerParams {
            tokio_handle: params.tokio_handle.clone(),
            event_bus: event_bus.clone(),
        }));
        let replicator = Rc::new(NetworkReplicator::new(client_api.clone()));

        // App data
        lua.set_app_data::<app_data::EventListeners>(app_data::EventListeners(HashMap::new()));
        lua.set_app_data::<app_data::Scenes>(app_data::Scenes(HashMap::new()));
        lua.set_app_data::<app_data::ScenePlugins>(app_data::ScenePlugins(HashMap::new()));
        lua.set_app_data::<app_data::Yielder>(app_data::Yielder(None));
        lua.set_app_data::<app_data::World>(app_data::World(hecs::World::new()));
        lua.set_app_data::<app_data::EventBus>(app_data::EventBus(event_bus.clone()));
        lua.set_app_data::<app_data::BlueprintRegistry>(BlueprintRegistry::new());
        lua.set_app_data::<app_data::InputEventRegistry>(InputEventRegistry::default());
        lua.set_app_data::<app_data::ExecutionContext>(ExecutionContext::Global);
        lua.set_app_data::<app_data::LayerRegistry>(LayerRegistry::new());
        lua.set_app_data::<app_data::ActiveLayers>(app_data::ActiveLayers(Vec::new()));
        lua.set_app_data::<app_data::ClientApi>(app_data::ClientApi(client_api.clone()));
        lua.set_app_data::<app_data::TimerManager>(app_data::TimerManager(timer_manager.clone()));
        lua.set_app_data::<app_data::NetworkReplicator>(app_data::NetworkReplicator(
            replicator.clone(),
        ));
        lua.set_app_data::<app_data::ReplicatorMarkTx>(app_data::ReplicatorMarkTx(
            replicator.get_mark_tx(),
        ));
        lua.set_app_data::<FieldRegistry>(FieldRegistry::new(&lua)?);
        lua.set_app_data::<app_data::EntityCustoms>(app_data::EntityCustoms(HashMap::new()));
        lua.set_app_data::<app_data::RoomIdToName>(app_data::RoomIdToName(HashMap::new()));

        // Plugins
        let (scene_manager_tx, scene_manager_rx) = flume::bounded::<SceneManagerMessage>(256);

        let mut plugin_composer = PluginComposer::new(&lua)?;
        let mut plugins: Vec<Box<dyn GameModePlugin>> = vec![
            Box::new(ConstantsPlugin {}),
            Box::new(JsonPlugin {}),
            Box::new(InputPlugin {}),
            Box::new(OnPlugin {
                descriptors: GLOBAL_EVENT_DESCRIPTORS,
            }),
            Box::new(EntityPlugin {}),
            Box::new(MemoryPlugin {
                meta_db: meta_db.clone(),
            }),
            Box::new(LayerPlugin {}),
            Box::new(PlayerPlugin {}),
            Box::new(TimerPlugin {}),
            Box::new(RoomPlugin {}),
            Box::new(ScenePlugin {
                manager_tx: scene_manager_tx.clone(),
            }),
        ];
        if let Some(fc_api) = params.fc_api {
            plugins.push(Box::new(FarcasterPlugin {
                fc_api: Arc::new(fc_api),
                meta_db: meta_db.clone(),
                config: params.config.farcaster,
            }));
        }
        if let Some(crypto) = params.crypto {
            plugins.push(Box::new(RockPlugin {
                crypto: Arc::new(crypto),
            }));
        }

        for plugin in plugins {
            plugin_composer.add_plugin(&lua, plugin)?;
        }

        let scene_manager = SceneManager::new(SceneManagerParams {
            plugins: plugin_composer.consume_plugins(),
            tx: scene_manager_tx,
            rx: scene_manager_rx,
            tokio_handle: params.tokio_handle,
        });

        let geodes = geode::scan_geodes()?;
        script::boot_gamemode(&lua, &params.config.gamemode.name, &geodes)
            .wrap_err("Failed to boot gamemode")?;

        Ok(Self {
            tick: 0,
            lua,
            callback_rx: params.callback_rx,
            command_rx: params.command_rx,
            commit_router: params.commit_router,
            scene_manager,
            event_bus,
            timer_manager,
            replicator,
        })
    }

    pub fn awaken(&mut self) -> eyre::Result<RuntimeExit> {
        self.event_bus.schedule_event(GameModeEvent {
            scopes: smallvec![EventScope::Global],
            data: GameModeEventData::World(WorldEventData::Awake),
        });

        let tick_interval = Duration::from_nanos(16_666_667);
        let mut next_tick = Instant::now();
        loop {
            if let Ok(cmd) = self.command_rx.try_recv() {
                match cmd {
                    RuntimeCommand::Reload => return Ok(RuntimeExit::Reload),
                    RuntimeCommand::Shutdown => return Ok(RuntimeExit::Shutdown),
                }
            }

            self.tick += 1;

            while let Ok(cb) = self.callback_rx.try_recv() {
                match cb {
                    RuntimeCallback::System(cb) => {
                        self.on_system_callback(cb);
                    }
                    RuntimeCallback::Client(cb) => {
                        if let Err(err) = self.on_client_request(cb) {
                            eprintln!("Failed to process a client message: {}", err);
                        };
                    }
                }
            }

            self.scene_manager.tick(&self.lua);
            self.timer_manager.tick();
            self.event_bus.flush(&self.lua)?;
            self.replicator.replicate(&self.lua, self.tick)?;

            let now = Instant::now();
            next_tick += tick_interval;
            if next_tick > now {
                std::thread::sleep(next_tick - now);
            } else {
                // Lag
                next_tick = now;
            }
        }
    }

    // Untrusted input (called by the client)
    fn on_client_request(&self, message: ClientRequest) -> eyre::Result<()> {
        let player = PlayerHandle::new(message.sender);

        match message.payload {
            IncomingRequest::Input(action) => {
                let action_name = self
                    .lua
                    .app_data_ref::<app_data::InputEventRegistry>()
                    .ok_or_else(|| eyre::eyre!("App data is not initialized"))?
                    .get_action_name(action.clone())?;
                self.event_bus.schedule_event(GameModeEvent {
                    scopes: smallvec![EventScope::Global],
                    data: GameModeEventData::Player(PlayerEventData::Input {
                        player,
                        name: action_name,
                        data: action.data,
                    }),
                });
            }
            IncomingRequest::Signal(signal) => {
                self.event_bus.schedule_event(GameModeEvent {
                    scopes: smallvec![EventScope::Global],
                    data: GameModeEventData::Player(PlayerEventData::Signal { player, signal }),
                });
            }
        }

        Ok(())
    }

    // Trusted input (called by the engine)
    fn on_system_callback(&self, cb: SystemCallback) {
        match cb {
            SystemCallback::PlayerConnect {
                pk,
                connection_params,
            } => {
                self.event_bus.schedule_event(GameModeEvent {
                    scopes: smallvec![EventScope::Global],
                    data: GameModeEventData::Player(PlayerEventData::Online {
                        player: PlayerHandle::new(pk),
                        connection_params,
                    }),
                });
            }
            SystemCallback::PlayerDisconnect { pk } => {
                self.event_bus.schedule_event(GameModeEvent {
                    scopes: smallvec![EventScope::Global],
                    data: GameModeEventData::Player(PlayerEventData::Offline {
                        player: PlayerHandle::new(pk),
                    }),
                });
            }
            SystemCallback::ImpromptuRequest { name, code } => {
                if let Err(err) = self.process_impromptu(ImpromptuRequest { name, code }) {
                    eprintln!("Faile to process an impromptu: {err}");
                }
            }
            SystemCallback::Webhook(event) => {
                self.event_bus.schedule_event(GameModeEvent {
                    scopes: smallvec![EventScope::Global],
                    data: GameModeEventData::Farcaster(FarcasterEventData::Webhook(event)),
                });
            }
        }
    }

    fn process_impromptu(&self, impromptu: ImpromptuRequest) -> mlua::Result<()> {
        self.event_bus.schedule_event(GameModeEvent {
            scopes: smallvec![EventScope::Global],
            data: GameModeEventData::World(WorldEventData::Impromptu {
                name: impromptu.name.clone(),
            }),
        });

        let name = impromptu.name.as_deref().unwrap_or("anonymous impromptu");

        let env = self.lua.create_table()?;
        env.set("_G", env.clone())?;

        let globals = self.lua.globals();
        let mt = self.lua.create_table()?;
        mt.set("__index", globals)?;
        env.set_metatable(Some(mt))?;

        self.lua
            .set_app_data::<app_data::ExecutionContext>(ExecutionContext::Impromptu);
        let result = self
            .lua
            .load(impromptu.code)
            .set_name(name)
            .set_environment(env)
            .exec();
        self.lua
            .set_app_data::<app_data::ExecutionContext>(ExecutionContext::Global);

        result?;
        Ok(())
    }
}
