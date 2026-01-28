use std::{
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use color_eyre::eyre::{self};
use mlua::Lua;
use shared::GameModeRequest;

use crate::{
    gamemode::{
        api::{
            memory::MemoryPlugin, protocol::GameModePlugin, scene::ScenePlugin, when::WhenPlugin,
        },
        scheduler::{Scheduler, SchedulerParams},
    },
    meta_db::MetaDb,
    router::CommitRouter,
    world::{WorldNatives, WorldState},
};

pub mod default_event_listener;
pub mod protocol;
pub mod scheduler;
pub use protocol::*;

mod app_data;
mod utils;
use app_data::GameModeAppData;
use utils::LuaResultExt;

mod api;

const LUA_STDLIB: &str = include_str!("gamemode/lua/stdlib.lua");

pub trait GameModeEventListener {
    fn emit(&self, event: GameModeEvent);
}

pub struct GameModeParams {
    pub name: String,
    pub event_listener: Box<dyn GameModeEventListener>,
    pub callback_rx: flume::Receiver<GameModeCallback>,
    pub commit_router: CommitRouter,
    pub meta_db: MetaDb,
}

pub struct GameMode {
    lua: Lua,
    event_listener: Box<dyn GameModeEventListener>,
    callback_rx: flume::Receiver<GameModeCallback>,
    world_state: Rc<WorldState>,
    world_natives: WorldNatives,
    commit_router: CommitRouter,
    meta_db: Arc<MetaDb>,
    scheduler: Scheduler,
}
impl GameMode {
    pub fn new(params: GameModeParams) -> eyre::Result<Self> {
        let meta_db = Arc::new(params.meta_db);
        let world_state = Rc::new(WorldState::new());
        let world_natives = WorldNatives {
            state: world_state.clone(),
        };

        let lua = Lua::new();
        let script_path = format!("gamemodes/{}.lua", params.name);
        let script = std::fs::read_to_string(script_path)?;

        let app_data = GameModeAppData {
            world_awakes: None,
            scenes: HashMap::new(),
            scene_plugins: HashMap::new(),
            yielder: None,
        };
        lua.set_app_data(app_data);

        // lua.load(LUA_STDLIB)
        //     .exec()
        //     .wrap_err("`stdlib` injection error")?;

        // Plugins
        let plugins: Vec<Box<dyn GameModePlugin>> = vec![
            Box::new(WhenPlugin {}),
            Box::new(MemoryPlugin {
                meta_db: meta_db.clone(),
            }),
            Box::new(ScenePlugin {}),
        ];
        let registered_plugins_map = api::register(&lua, plugins)?;

        let scheduler = Scheduler::new(SchedulerParams {
            channel_buffer: 1024,
            plugins: registered_plugins_map,
        });

        // Load script
        lua.load(&script)
            .exec()
            .wrap_err("Script execution error")?;

        Ok(Self {
            lua,
            event_listener: params.event_listener,
            callback_rx: params.callback_rx,
            world_state,
            world_natives,
            commit_router: params.commit_router,
            meta_db,
            scheduler,
        })
    }

    pub fn awaken(&self) -> eyre::Result<Self> {
        // Call when.world.awakes
        if let Some(when_world_awakes) =
            self.lua
                .app_data_ref::<GameModeAppData>()
                .and_then(|app_data| {
                    app_data
                        .world_awakes
                        .as_ref()
                        .and_then(|rk| self.lua.registry_value::<mlua::Function>(rk).ok())
                })
        {
            when_world_awakes
                .call::<()>(())
                .wrap_err("Error in `when.world.awakes`")?;
        }

        let tick_interval = Duration::from_nanos(16_666_667);
        let mut next_tick = Instant::now();
        loop {
            while let Ok(cb) = self.callback_rx.try_recv() {
                match cb {
                    GameModeCallback::Engine(cb) => {
                        self.on_engine_callback(cb);
                    }
                    GameModeCallback::Client(cb) => {
                        self.on_client_request(cb);
                    }
                    GameModeCallback::Indexer(_) => {}
                }
            }

            // Lua callbacks
            // OnTick callback (at least)

            // Read your Writes err
            // Lua sends a game intent then can't get the result in the same tick

            // world.step
            // Also, subscribe Lua to CommitRouter

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
    fn on_client_request(&self, message: ClientRequest) {
        println!("[gamemode] new client message: {:?}", message);
        self.event_listener.emit(GameModeEvent::SendClientMessage {
            pk: message.sender,
            text: String::from("Hello from Wonderful RP!"),
        });

        match message.payload {
            GameModeRequest::PlayerMove(dir) => {
                // TODO: Who's being moved? How?
                println!("[CLIENT] PlayerMove: {:?}", dir);
            }
        }
    }

    // Trusted input (called by the engine)
    fn on_engine_callback(&self, cb: EngineCallback) {
        match cb {
            EngineCallback::OnGameModeInit => {
                println!("[gamemode] gamemode init");
                // Load the world, initialize entities
            }
            EngineCallback::OnPlayerConnect { pk } => {
                println!("[gamemode] player connected: {:?}", pk);
                // Spawn player, include the player into the world
            }
        }
    }
}
