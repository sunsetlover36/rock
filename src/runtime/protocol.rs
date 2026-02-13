use color_eyre::eyre;
use shared::{GameModeClientRequest, PlayerKey};

use crate::{
    actor::indexer::protocol::IndexerEvent, envelope::ClientEnvelope, runtime::utils::LuaResultExt,
};

pub type ClientRequest = ClientEnvelope<GameModeClientRequest>;

#[derive(Debug, Clone)]
pub enum GameModeClientCommand {
    SendMessage { pk: PlayerKey, text: String },
    Broadcast { text: String },
    Log { text: String },
    KickPlayer { pk: PlayerKey },
}
pub trait GameModeClientApi: Send {
    fn send(&self, event: GameModeClientCommand);
}

pub enum SystemCallback {
    OnPlayerConnect { pk: PlayerKey },
}
pub enum RuntimeCallback {
    System(SystemCallback),
    Client(ClientRequest),
    Indexer(IndexerEvent),
}

// Lua event listeners (gamemode events)
#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub enum WorldEvent {
    Awake,
}

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub enum PlayerEvent {
    Connect,
}

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub enum GameModeEvent {
    World(WorldEvent),
    Player(PlayerEvent),
}

pub struct GameModeListener {
    pub name: Option<String>,
    pub handle: mlua::Function,
    pub call_count: u32,
    pub limit: Option<u32>,
    pub filters: Vec<mlua::Function>,
}
impl GameModeListener {
    pub fn limit_reached(&self) -> bool {
        match self.limit {
            Some(limit) => limit == self.call_count,
            None => false,
        }
    }
    pub fn passes_filters(&self, args: &mlua::MultiValue) -> eyre::Result<bool> {
        self.filters.iter().try_fold(true, |_, filter| {
            filter
                .call::<bool>(args)
                .wrap_err("Error when filtering a chain for the event listener")
        })
    }
}

pub struct GameModeEntity {
    name: String,
    components: Vec<ComponentVariant>,
    custom_data: Option<mlua::Table>,
}
