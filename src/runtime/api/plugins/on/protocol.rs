use color_eyre::eyre;

use crate::runtime::{
    api::plugins::entity::{ComponentData, ComponentKey},
    utils::LuaResultExt,
};

pub(crate) struct EventDescriptor {
    pub namespace: Option<&'static str>,
    pub name: &'static str,
    pub event_key: GameModeEventKey,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum EventScope {
    Global,
    Entity(u64),
    Blueprint(u64),
}

pub struct GameModeListener {
    pub name: Option<String>,
    pub scope: EventScope,
    pub handle: mlua::Function,
    pub call_count: u32,
    pub limit: Option<u32>,
    pub predicates: Vec<mlua::Function>,
}
impl GameModeListener {
    pub fn limit_reached(&self) -> bool {
        match self.limit {
            Some(limit) => limit == self.call_count,
            None => false,
        }
    }
    pub fn passes_filters(&self, args: &mlua::MultiValue) -> eyre::Result<bool> {
        self.predicates.iter().try_fold(true, |_, predicate| {
            predicate
                .call::<bool>(args)
                .wrap_err("Error when filtering a chain for the event listener")
        })
    }
}

// keys
#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub(crate) enum WorldEventKey {
    Awake,
}

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub(crate) enum PlayerEventKey {
    Connect,
}

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub(crate) enum EntityEventKey {
    ComponentUpdate(ComponentKey),
}

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub(crate) enum GameModeEventKey {
    World(WorldEventKey),
    Player(PlayerEventKey),
    Entity(EntityEventKey),
}

// payloads
pub(crate) enum WorldEventData {
    Awake,
}
impl WorldEventData {
    pub fn kind(&self) -> WorldEventKey {
        match self {
            WorldEventData::Awake => WorldEventKey::Awake,
        }
    }
}

pub(crate) enum PlayerEventData {
    Connect { id: u32 },
}
impl PlayerEventData {
    pub fn kind(&self) -> PlayerEventKey {
        match self {
            PlayerEventData::Connect { .. } => PlayerEventKey::Connect,
        }
    }
}

pub(crate) enum EntityEventData {
    ComponentUpdate(ComponentData),
}
impl EntityEventData {
    pub fn kind(&self) -> EntityEventKey {
        match self {
            EntityEventData::ComponentUpdate(data) => EntityEventKey::ComponentUpdate(data.into()),
        }
    }
}

pub(crate) enum GameModeEventData {
    World(WorldEventData),
    Player(PlayerEventData),
    Entity(EntityEventData),
}
impl GameModeEventData {
    pub fn kind(&self) -> GameModeEventKey {
        match self {
            GameModeEventData::World(e) => GameModeEventKey::World(e.kind()),
            GameModeEventData::Player(e) => GameModeEventKey::Player(e.kind()),
            GameModeEventData::Entity(e) => GameModeEventKey::Entity(e.kind()),
        }
    }
}
