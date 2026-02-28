use std::{
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use color_eyre::eyre::{self};
use mlua::Lua;
use shared::{ImpromptuRequest, IncomingRequest};
use smallvec::smallvec;

use crate::{
    meta_db::MetaDb,
    router::CommitRouter,
    runtime::{
        api::{
            InputPlugin, LayerPlugin, SceneManagerParams,
            on::{
                EventScope, GameModeEvent, GameModeEventData, OnPlugin, PlayerEventData,
                WorldEventData, event_descriptors::GLOBAL_EVENT_DESCRIPTORS,
            },
            protocol::GameModePlugin,
        },
        app_data::{ExecutionContext, InputEventRegistry},
    },
    world::{WorldNatives, WorldState},
};

pub mod default_client_api;
pub(crate) mod event_bus;
pub(crate) use event_bus::EventBus;

mod api;
use api::{
    EntityPlugin, MemoryPlugin, PluginComposer, SceneManager, SceneManagerMessage, ScenePlugin,
};

mod app_data;

mod geode;
use geode::{inject_geodes, scan_geodes};

pub mod protocol;
pub use protocol::*;

mod utils;
use utils::LuaResultExt;

pub struct RuntimeParams {
    pub name: String,
    pub client_api: Box<dyn GameModeClientApi>,
    pub callback_rx: flume::Receiver<RuntimeCallback>,
    pub commit_router: CommitRouter,
    pub meta_db: MetaDb,
    pub tokio_handle: tokio::runtime::Handle,
}

pub struct Runtime {
    lua: Lua,
    client_api: Box<dyn GameModeClientApi>,
    callback_rx: flume::Receiver<RuntimeCallback>,
    world_state: Rc<WorldState>,
    world_natives: WorldNatives,
    commit_router: CommitRouter,
    meta_db: Arc<MetaDb>,
    scene_manager: SceneManager,
    event_bus: Rc<EventBus>,
}
impl Runtime {
    pub fn new(params: RuntimeParams) -> eyre::Result<Self> {
        let lua = Lua::new();

        // Dependencies
        let meta_db = Arc::new(params.meta_db);
        let world_state = Rc::new(WorldState::new());
        let world_natives = WorldNatives {
            state: world_state.clone(),
        };
        let event_bus = Rc::new(EventBus::new());

        // App data
        lua.set_app_data::<app_data::EventListeners>(HashMap::new());
        lua.set_app_data::<app_data::Scenes>(HashMap::new());
        lua.set_app_data::<app_data::ScenePlugins>(HashMap::new());
        lua.set_app_data::<app_data::Yielder>(None);
        lua.set_app_data::<app_data::World>(hecs::World::new());
        lua.set_app_data::<app_data::EventBus>(event_bus.clone());
        lua.set_app_data::<app_data::Blueprints>(HashMap::new());
        lua.set_app_data::<app_data::InputEventRegistry>(InputEventRegistry::default());
        lua.set_app_data::<app_data::ExecutionContext>(ExecutionContext::Global);
        lua.set_app_data::<app_data::LayerCleaners>(HashMap::new());
        lua.set_app_data::<app_data::ActiveLayers>(Vec::new());

        // Plugins
        let (scene_manager_tx, scene_manager_rx) = flume::bounded::<SceneManagerMessage>(256);

        let mut plugin_composer = PluginComposer::new(&lua)?;
        let plugins: Vec<Box<dyn GameModePlugin>> = vec![
            Box::new(InputPlugin {}),
            Box::new(OnPlugin {
                descriptors: GLOBAL_EVENT_DESCRIPTORS,
            }),
            Box::new(EntityPlugin {}),
            Box::new(MemoryPlugin {
                meta_db: meta_db.clone(),
            }),
            Box::new(LayerPlugin {}),
            Box::new(ScenePlugin {
                manager_tx: scene_manager_tx.clone(),
            }),
        ];
        for plugin in plugins {
            plugin_composer.add_plugin(&lua, plugin)?;
        }

        let scene_manager = SceneManager::new(SceneManagerParams {
            plugins: plugin_composer.consume_plugins(),
            tx: scene_manager_tx,
            rx: scene_manager_rx,
            tokio_handle: params.tokio_handle,
        });

        // Geodes injection
        inject_geodes(&lua, &scan_geodes()?)?;

        // Gamemode script string
        let gamemode_path = format!("gamemodes/{}.lua", params.name);
        let gamemode = std::fs::read_to_string(&gamemode_path)?;
        lua.load(&gamemode)
            .exec()
            .wrap_err("Script execution error")?;

        Ok(Self {
            lua,
            client_api: params.client_api,
            callback_rx: params.callback_rx,
            world_state,
            world_natives,
            commit_router: params.commit_router,
            meta_db,
            scene_manager,
            event_bus,
        })
    }

    pub fn awaken(&mut self) -> eyre::Result<Self> {
        self.event_bus.schedule_event(GameModeEvent {
            scopes: smallvec![EventScope::Global],
            data: GameModeEventData::World(WorldEventData::Awake),
        });

        let tick_interval = Duration::from_nanos(16_666_667);
        let mut next_tick = Instant::now();
        loop {
            self.scene_manager.tick(&self.lua);

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

            // Physics step

            self.event_bus.flush(&self.lua)?;

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
        let id = message.sender.pack();

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
                        id,
                        name: action_name,
                        data: action.data,
                    }),
                });
            }
        }

        Ok(())
    }

    // Trusted input (called by the engine)
    fn on_system_callback(&self, cb: SystemCallback) {
        match cb {
            SystemCallback::OnPlayerConnect { pk } => {
                self.event_bus.schedule_event(GameModeEvent {
                    scopes: smallvec![EventScope::Global],
                    data: GameModeEventData::Player(PlayerEventData::Connect { id: pk.pack() }),
                });
            }
            SystemCallback::OnPlayerDisconnect { pk } => {
                self.event_bus.schedule_event(GameModeEvent {
                    scopes: smallvec![EventScope::Global],
                    data: GameModeEventData::Player(PlayerEventData::Disconnect { id: pk.pack() }),
                });
            }
            SystemCallback::OnImpromptuRequest { name, code } => {
                self.process_impromptu(ImpromptuRequest { name, code });
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
