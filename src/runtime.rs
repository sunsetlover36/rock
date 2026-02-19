use std::{
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use color_eyre::eyre::{self};
use mlua::Lua;
use shared::GameModeClientRequest;

use crate::{
    meta_db::MetaDb,
    router::CommitRouter,
    runtime::api::on::{GameModeEventData, PlayerEventData, WorldEventData},
    world::{WorldNatives, WorldState},
};

pub mod default_client_api;
pub(crate) mod event_bus;
pub(crate) use event_bus::EventBus;

mod api;
use api::{ApiRegisterParams, SceneManager};

mod app_data;
use app_data::GameModeAppData;

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

        let meta_db = Arc::new(params.meta_db);
        let world_state = Rc::new(WorldState::new());
        let world_natives = WorldNatives {
            state: world_state.clone(),
        };
        let event_bus = Rc::new(EventBus::new());

        let app_data = GameModeAppData {
            event_listeners: HashMap::new(),
            scenes: HashMap::new(),
            scene_plugins: HashMap::new(),
            yielder: None,
            world: hecs::World::new(),
            event_bus: event_bus.clone(),
        };
        lua.set_app_data(app_data);

        // Plugins
        let scene_manager = api::register(
            &lua,
            ApiRegisterParams {
                tokio_handle: params.tokio_handle,
                scene_manager_channel_buffer: 256,
                meta_db: meta_db.clone(),
            },
        )?;

        // Gamemode script string
        inject_geodes(&lua, scan_geodes()?)?;
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
        self.event_bus
            .schedule_event(GameModeEventData::World(WorldEventData::Awake));

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
                        self.on_client_request(cb);
                    }
                }
            }

            // Lua callbacks
            // OnTick callback (at least)

            // Read your Writes err
            // Lua sends a game intent then can't get the result in the same tick

            // world.step

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
    fn on_client_request(&self, message: ClientRequest) {
        println!("[gamemode] new client message: {:?}", message);
        self.client_api.send(GameModeClientCommand::SendMessage {
            pk: message.sender,
            text: String::from("Hello from Wonderful RP!"),
        });

        match message.payload {
            GameModeClientRequest::PlayerMove(dir) => {
                // TODO: Who's being moved? How?
                println!("[CLIENT] PlayerMove: {:?}", dir);
            }
        }
    }

    // Trusted input (called by the engine)
    fn on_system_callback(&self, cb: SystemCallback) {
        match cb {
            SystemCallback::OnPlayerConnect { pk } => {
                self.event_bus.schedule_event(GameModeEventData::Player(
                    PlayerEventData::Connect { id: pk.pack() },
                ));
            }
            SystemCallback::OnPlayerDisconnect { pk } => {
                self.event_bus.schedule_event(GameModeEventData::Player(
                    PlayerEventData::Disconnect { id: pk.pack() },
                ));
            }
        }
    }
}
