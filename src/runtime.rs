use std::{
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use color_eyre::eyre::{self};
use mlua::{IntoLuaMulti, Lua};
use shared::GameModeClientRequest;

use crate::{
    meta_db::MetaDb,
    router::CommitRouter,
    world::{WorldNatives, WorldState},
};

pub mod default_client_api;
pub mod protocol;
pub use protocol::*;

mod app_data;
mod utils;
use app_data::GameModeAppData;
use utils::LuaResultExt;
mod api;
use api::{ApiRegisterParams, scheduler::Scheduler};

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
    scheduler: Scheduler,
}
impl Runtime {
    pub fn new(params: RuntimeParams) -> eyre::Result<Self> {
        let meta_db = Arc::new(params.meta_db);
        let world_state = Rc::new(WorldState::new());
        let world_natives = WorldNatives {
            state: world_state.clone(),
        };

        let lua = Lua::new();
        let script_path = format!("gamemodes/{}.lua", params.name);
        let script = std::fs::read_to_string(script_path)?;

        let app_data = GameModeAppData {
            event_listeners: HashMap::new(),
            scenes: HashMap::new(),
            scene_plugins: HashMap::new(),
            yielder: None,
        };
        lua.set_app_data(app_data);

        // Plugins
        let scheduler = api::register(
            &lua,
            ApiRegisterParams {
                tokio_handle: params.tokio_handle,
                scheduler_channel_buffer: 256,
                meta_db: meta_db.clone(),
            },
        )?;

        // Load script
        lua.load(&script)
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
            scheduler,
        })
    }

    // TODO: implement event args validation
    fn validate_event_args(&self, event: GameModeEvent, args: mlua::MultiValue) {
        match event {
            GameModeEvent::World(event) => match event {
                WorldEvent::Awake => {}
            },
            GameModeEvent::Player(event) => match event {
                PlayerEvent::Connect => {}
            },
        }
    }
    fn notify_event_listeners(
        &self,
        event: GameModeEvent,
        args: impl IntoLuaMulti,
    ) -> eyre::Result<()> {
        let args = args
            .into_lua_multi(&self.lua)
            .wrap_err("Failed to materialize args")?;

        let mut pending = Vec::new();
        {
            let mut app_data = match self.lua.app_data_mut::<GameModeAppData>() {
                Some(d) => d,
                None => return Err(eyre::eyre!("App data is not initialized")),
            };
            let listeners = match app_data.event_listeners.get_mut(&event) {
                Some(fns) => fns,
                None => return Ok(()),
            };

            for (id, listener) in listeners.iter().enumerate() {
                if listener.limit_reached() || !listener.passes_filters(&args)? {
                    continue;
                }

                pending.push((id, listener.handle.clone()))
            }
        }

        for (_, handle) in &pending {
            handle
                .call::<()>(&args)
                .wrap_err(format!("Error in `{:?}` event listener", event).as_str())?;
        }

        let mut app_data = match self.lua.app_data_mut::<GameModeAppData>() {
            Some(d) => d,
            None => return Err(eyre::eyre!("App data is not initialized")),
        };
        let listeners = match app_data.event_listeners.get_mut(&event) {
            Some(fns) => fns,
            None => {
                return Err(eyre::eyre!(
                    "Failed to increment call counts for event listeners, because `event_listeners` doesn't exist"
                ));
            }
        };
        for (id, _) in pending {
            listeners[id].call_count += 1;
        }
        listeners.retain(|l| !l.limit_reached());

        Ok(())
    }

    pub fn awaken(&mut self) -> eyre::Result<Self> {
        self.notify_event_listeners(GameModeEvent::World(WorldEvent::Awake), ())?;

        let tick_interval = Duration::from_nanos(16_666_667);
        let mut next_tick = Instant::now();
        loop {
            self.scheduler.tick(&self.lua);

            while let Ok(cb) = self.callback_rx.try_recv() {
                match cb {
                    RuntimeCallback::System(cb) => {
                        self.on_system_callback(cb);
                    }
                    RuntimeCallback::Client(cb) => {
                        self.on_client_request(cb);
                    }
                    RuntimeCallback::Indexer(_) => {}
                }
            }

            // Lua callbacks
            // OnTick callback (at least)

            // Read your Writes err
            // Lua sends a game intent then can't get the result in the same tick

            // world.step

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
                println!("[gamemode] player connected: {:?}", pk);
                // Spawn player, include the player into the world
            }
        }
    }
}
