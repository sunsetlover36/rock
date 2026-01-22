use std::time::{Duration, Instant};

use color_eyre::eyre::{self, Result};
use mlua::{Lua, UserData, UserDataMethods};
use shared::GameModeRequest;

use crate::actor::world::{GameIntent, WorldGetters};

pub mod default_event_listener;
pub mod protocol;
pub use protocol::*;

pub trait GameModeEventListener: Send + Sync {
    fn on_emit(&self, event: GameModeEvent);
}

pub struct GameModeParams {
    pub gamemode_name: String,
    pub gamemode_event_listener: Box<dyn GameModeEventListener>,
    pub gamemode_callback_rx: flume::Receiver<GameModeCallback>,
    pub game_intent_tx: flume::Sender<GameIntent>,
    pub world_getters: WorldGetters,
}

pub struct GameMode {
    lua: Lua,
    gamemode_event_listener: Box<dyn GameModeEventListener>,
    gamemode_callback_rx: flume::Receiver<GameModeCallback>,
    game_intent_tx: flume::Sender<GameIntent>,
    world_getters: WorldGetters,
}
impl GameMode {
    pub fn new(params: GameModeParams) -> Result<Self> {
        let lua = Lua::new();
        let script_path = format!("{}.lua", params.gamemode_name);
        let script = std::fs::read_to_string(script_path)?;

        lua.load(&script)
            .exec()
            .map_err(|e| eyre::eyre!("Lua script error: {}", e))?;

        Ok(Self {
            lua,
            gamemode_event_listener: params.gamemode_event_listener,
            gamemode_callback_rx: params.gamemode_callback_rx,
            game_intent_tx: params.game_intent_tx,
            world_getters: params.world_getters,
        })
    }

    pub fn awaken(&self) {
        // Call OnGameModeAwake (Lua callback)
        if let Ok(cb) = self.lua.globals().get::<mlua::Function>("OnGameModeInit") {
            let _ = cb.call::<()>(());
        }

        let tick_interval = Duration::from_nanos(16_666_667);
        let mut next_tick = Instant::now();
        loop {
            while let Ok(cb) = self.gamemode_callback_rx.try_recv() {
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
        self.gamemode_event_listener
            .on_emit(GameModeEvent::SendClientMessage {
                pk: message.sender,
                text: String::from("Hello from Wonderful RP!"),
            });

        match message.payload {
            GameModeRequest::PlayerMove(dir) => {
                // TODO: Who's being moved?
                self.game_intent_tx.send(GameIntent::MovePlayer(dir));
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

impl UserData for GameMode {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("Test", |_, ()| Ok(0));
    }
}
