use super::{
    plugins::{
        on::protocol::{
            EventScope, GameModeEvent, GameModeEventData, PlayerEventData, WorldEventData,
        },
        protocol::{AsyncTask, AsyncTaskResult, GameModePlugin, PluginName},
    },
    *,
};
use crate::socket::protocol::ServerMessage;
use parking_lot::Mutex;
use rock_wire::{PlayerKey, farcaster::Fid};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[derive(Default)]
pub(crate) struct Clients {
    pub online: Mutex<HashSet<PlayerKey>>,
    pub sent: AtomicUsize,
}
impl GameModeClientApi for Clients {
    fn has(&self, pk: PlayerKey) -> bool {
        self.online.lock().contains(&pk)
    }
    fn list(&self) -> Vec<PlayerKey> {
        self.online.lock().iter().copied().collect()
    }
    fn identity(&self, pk: PlayerKey) -> Option<String> {
        self.has(pk).then(|| format!("player:{}", pk.pack()))
    }
    fn fid(&self, _: PlayerKey) -> Option<Fid> {
        None
    }
    fn send(&self, _: ServerMessage) {
        self.sent.fetch_add(1, Ordering::SeqCst);
    }
    fn kick(&self, pk: PlayerKey) {
        self.online.lock().remove(&pk);
    }
}

struct PendingMemory {
    completed: Arc<AtomicUsize>,
    dropped: Arc<AtomicUsize>,
}
struct Dropped(Arc<AtomicUsize>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl GameModePlugin for PendingMemory {
    fn name(&self) -> PluginName {
        PluginName::Memory
    }
    fn create_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let table = lua.create_table()?;
        table.set(
            "fetch",
            lua.create_async_function(async |lua, key: String| {
                let op = plugins::build_plugin_op(
                    &lua,
                    "MEMORY_FETCH",
                    mlua::Value::String(lua.create_string(&key)?),
                )?;
                plugins::yield_op(&lua, "memory.fetch", op).await
            })?,
        )?;
        Ok(Some(table))
    }
    fn handle_op(
        &self,
        _: &mlua::Lua,
        _: &str,
        _: mlua::Value,
    ) -> color_eyre::eyre::Result<Option<AsyncTask>> {
        let completed = self.completed.clone();
        let guard = Dropped(self.dropped.clone());
        Ok(Some(Box::pin(async move {
            let _guard = guard;
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            completed.fetch_add(1, Ordering::SeqCst);
            Ok(AsyncTaskResult::JsonValue(
                serde_json::json!({ "health": 100 }),
            ))
        })))
    }
}

pub(crate) struct Fixture {
    pub lua: mlua::Lua,
    pub bus: Rc<EventBus>,
    pub timers: Rc<TimerManager>,
    pub scenes: SceneManager,
    pub scene_tx: flume::Sender<SceneManagerMessage>,
    pub clients: Arc<Clients>,
    pub completed: Arc<AtomicUsize>,
    pub dropped: Arc<AtomicUsize>,
}
impl Fixture {
    pub fn new() -> Self {
        let lua = mlua::Lua::new();
        let bus = Rc::new(EventBus::new());
        let clients = Arc::new(Clients::default());
        let timers = Rc::new(TimerManager::new(TimerManagerParams {
            tokio_handle: tokio::runtime::Handle::current(),
            event_bus: bus.clone(),
        }));
        let (scene_tx, rx) = flume::unbounded();
        let completed = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicUsize::new(0));
        lua.set_app_data(lifetime::ActivePlayers::default());
        lua.set_app_data(lifetime::PlayerWorkRegistry::default());
        lua.set_app_data(app_data::EventListeners(HashMap::new()));
        lua.set_app_data(app_data::Scenes(HashMap::new()));
        lua.set_app_data(app_data::World(hecs::World::new()));
        lua.set_app_data(app_data::EntityCustoms(HashMap::new()));
        lua.set_app_data(app_data::BlueprintRegistry::new());
        lua.set_app_data(app_data::LayerRegistry::new());
        lua.set_app_data(app_data::ActiveLayers(Vec::new()));
        lua.set_app_data(app_data::ExecutionContext::Global);
        lua.set_app_data(app_data::ClientApi(clients.clone()));
        lua.set_app_data(app_data::TimerManager(timers.clone()));
        lua.set_app_data(app_data::EventBus(bus.clone()));
        lua.set_app_data(FieldRegistry::new(&lua).unwrap());
        let mut async_plugins: HashMap<PluginName, Box<dyn GameModePlugin>> = HashMap::new();
        let memory = Box::new(PendingMemory {
            completed: completed.clone(),
            dropped: dropped.clone(),
        });
        lua.globals()
            .set("memory", memory.create_api(&lua).unwrap().unwrap())
            .unwrap();
        async_plugins.insert(PluginName::Memory, memory);
        for plugin in [
            Box::new(LayerPlugin {}) as Box<dyn GameModePlugin>,
            Box::new(EntityPlugin {}),
            Box::new(OnPlugin {
                descriptors: GLOBAL_EVENT_DESCRIPTORS,
            }),
            Box::new(TimerPlugin {}),
            Box::new(ScenePlugin {
                manager_tx: scene_tx.clone(),
            }),
            Box::new(PlayerPlugin {}),
        ] {
            lua.globals()
                .set(
                    plugin.name().to_string(),
                    plugin.create_api(&lua).unwrap().unwrap(),
                )
                .unwrap();
        }
        let disconnect_clients = clients.clone();
        lua.globals()
            .set(
                "disconnect",
                lua.create_function(
                    move |lua, p: mlua::UserDataRef<plugins::player::PlayerHandle>| {
                        disconnect_clients.online.lock().remove(&p.key());
                        lifetime::cancel_player(lua, p.key());
                        Ok(())
                    },
                )
                .unwrap(),
            )
            .unwrap();
        lua.load("results = { calls = 0 }").exec().unwrap();
        let scenes = SceneManager::new(SceneManagerParams {
            plugins: async_plugins,
            tx: scene_tx.clone(),
            rx,
            tokio_handle: tokio::runtime::Handle::current(),
        });
        Self {
            lua,
            bus,
            timers,
            scenes,
            scene_tx,
            clients,
            completed,
            dropped,
        }
    }
    pub fn eval(&self, script: &str) {
        self.lua.load(script).exec().unwrap();
    }
    pub fn connect(&self, pk: PlayerKey) {
        self.clients.online.lock().insert(pk);
        self.lua
            .globals()
            .set("p", plugins::player::PlayerHandle::new(pk))
            .unwrap();
    }
    pub fn online(&self, pk: PlayerKey) {
        self.connect(pk);
        self.bus.schedule_event(GameModeEvent {
            scopes: smallvec![EventScope::Global],
            data: GameModeEventData::Player(PlayerEventData::Online {
                player: plugins::player::PlayerHandle::new(pk),
                connection_params: HashMap::new(),
            }),
        });
    }
    pub fn disconnect(&self, pk: PlayerKey) {
        self.clients.online.lock().remove(&pk);
        lifetime::cancel_player(&self.lua, pk);
        self.bus.schedule_event(GameModeEvent {
            scopes: smallvec![EventScope::Global],
            data: GameModeEventData::Player(PlayerEventData::Offline {
                player: PlayerSnapshot::new(pk, Some("saved-identity".into())),
            }),
        });
    }
    pub fn fire(&self, name: &str) {
        self.bus.schedule_event(GameModeEvent {
            scopes: smallvec![EventScope::Global],
            data: GameModeEventData::World(WorldEventData::Impromptu {
                name: Some(name.into()),
            }),
        });
    }
    pub fn tick(&mut self) {
        self.scenes.tick(&self.lua);
        self.timers.tick(&self.lua);
        self.bus.flush(&self.lua).unwrap();
    }
    pub async fn settle(&mut self) {
        for _ in 0..3 {
            tokio::task::yield_now().await;
            self.tick();
        }
    }
    pub async fn advance(&mut self, seconds: u64) {
        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_secs(seconds)).await;
        self.settle().await;
    }
    pub fn calls(&self) -> usize {
        self.lua
            .globals()
            .get::<mlua::Table>("results")
            .unwrap()
            .get("calls")
            .unwrap()
    }
}
