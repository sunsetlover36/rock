use mlua::{IntoLuaMulti, LuaSerdeExt};

use crate::runtime::api::plugins::entity::{ComponentData, ComponentKey};

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

// Event keys
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
    CustomDataUpdate,
}

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub(crate) enum GameModeEventKey {
    World(WorldEventKey),
    Player(PlayerEventKey),
    Entity(EntityEventKey),
}

// Event payloads
pub(crate) enum WorldEventData {
    Awake,
}
impl WorldEventData {
    pub fn key(&self) -> WorldEventKey {
        match self {
            WorldEventData::Awake => WorldEventKey::Awake,
        }
    }
}
impl IntoLuaMulti for WorldEventData {
    fn into_lua_multi(self, lua: &mlua::Lua) -> mlua::Result<mlua::MultiValue> {
        match self {
            WorldEventData::Awake => ().into_lua_multi(lua),
        }
    }
}

pub(crate) enum PlayerEventData {
    Connect { id: u32 },
}
impl PlayerEventData {
    pub fn key(&self) -> PlayerEventKey {
        match self {
            PlayerEventData::Connect { .. } => PlayerEventKey::Connect,
        }
    }
}
impl IntoLuaMulti for PlayerEventData {
    fn into_lua_multi(self, lua: &mlua::Lua) -> mlua::Result<mlua::MultiValue> {
        match self {
            PlayerEventData::Connect { id } => id.into_lua_multi(lua),
        }
    }
}

pub(crate) enum EntityEventData {
    ComponentUpdate(ComponentData),
    CustomDataUpdate(mlua::Table),
}
impl EntityEventData {
    pub fn key(&self) -> EntityEventKey {
        match self {
            EntityEventData::ComponentUpdate(data) => EntityEventKey::ComponentUpdate(data.into()),
            EntityEventData::CustomDataUpdate(_) => EntityEventKey::CustomDataUpdate,
        }
    }
}
impl IntoLuaMulti for EntityEventData {
    fn into_lua_multi(self, lua: &mlua::Lua) -> mlua::Result<mlua::MultiValue> {
        match self {
            EntityEventData::ComponentUpdate(data) => {
                Ok(mlua::MultiValue::from(vec![lua.to_value(&data)?]))
            }
            EntityEventData::CustomDataUpdate(data) => data.into_lua_multi(lua),
        }
    }
}

pub(crate) enum GameModeEventData {
    World(WorldEventData),
    Player(PlayerEventData),
    Entity(EntityEventData),
}
impl GameModeEventData {
    pub fn key(&self) -> GameModeEventKey {
        match self {
            GameModeEventData::World(e) => GameModeEventKey::World(e.key()),
            GameModeEventData::Player(e) => GameModeEventKey::Player(e.key()),
            GameModeEventData::Entity(e) => GameModeEventKey::Entity(e.key()),
        }
    }
}
impl IntoLuaMulti for GameModeEventData {
    fn into_lua_multi(self, lua: &mlua::Lua) -> mlua::Result<mlua::MultiValue> {
        match self {
            GameModeEventData::World(e) => e.into_lua_multi(lua),
            GameModeEventData::Player(e) => e.into_lua_multi(lua),
            GameModeEventData::Entity(e) => e.into_lua_multi(lua),
        }
    }
}
