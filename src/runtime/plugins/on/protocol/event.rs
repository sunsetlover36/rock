use mlua::{IntoLua, IntoLuaMulti};
use smallvec::SmallVec;

pub(crate) mod entity;
pub(crate) use entity::*;

pub(crate) mod farcaster;
pub(crate) use farcaster::*;

pub(crate) mod player;
pub(crate) use player::*;

pub(crate) mod timer;
pub(crate) use timer::*;

pub(crate) mod world;
pub(crate) use world::*;

pub(crate) struct EventDescriptor {
    pub namespace: Option<&'static str>,
    pub name: &'static str,
    pub event_key: GameModeEventKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EventScope {
    Global,
    Entity(u64),
    Blueprint(u64),
}

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub(crate) enum GameModeEventKey {
    World(WorldEventKey),
    Player(PlayerEventKey),
    Entity(EntityEventKey),
    Timer(TimerEventKey),
    Farcaster(FarcasterEventKey),
}

pub(crate) enum GameModeEventData {
    World(WorldEventData),
    Player(PlayerEventData),
    Entity(EntityEventData),
    Timer(TimerEventData),
    Farcaster(FarcasterEventData),
}
impl GameModeEventData {
    pub fn key(&self) -> GameModeEventKey {
        match self {
            GameModeEventData::World(e) => GameModeEventKey::World(e.key()),
            GameModeEventData::Player(e) => GameModeEventKey::Player(e.key()),
            GameModeEventData::Entity(e) => GameModeEventKey::Entity(e.key()),
            GameModeEventData::Timer(e) => GameModeEventKey::Timer(e.key()),
            GameModeEventData::Farcaster(e) => GameModeEventKey::Farcaster(e.key()),
        }
    }
}
impl IntoLuaMulti for GameModeEventData {
    fn into_lua_multi(self, lua: &mlua::Lua) -> mlua::Result<mlua::MultiValue> {
        match self {
            GameModeEventData::World(e) => e.into_lua_multi(lua),
            GameModeEventData::Player(e) => e.into_lua_multi(lua),
            GameModeEventData::Entity(e) => e.into_lua_multi(lua),
            GameModeEventData::Timer(e) => e.into_lua_multi(lua),
            GameModeEventData::Farcaster(e) => e.into_lua_multi(lua),
        }
    }
}

pub(crate) struct GameModeEvent {
    pub scopes: SmallVec<[EventScope; 2]>,
    pub data: GameModeEventData,
}
impl IntoLuaMulti for GameModeEvent {
    fn into_lua_multi(self, lua: &mlua::Lua) -> mlua::Result<mlua::MultiValue> {
        let entity_id = self.scopes.iter().find_map(|scope| {
            if let EventScope::Entity(id) = scope {
                Some(id.clone())
            } else {
                None
            }
        });

        let mut lua_args = self.data.into_lua_multi(lua)?.into_vec();
        if let Some(id) = entity_id {
            lua_args.insert(0, id.into_lua(lua)?);
        }

        Ok(mlua::MultiValue::from_vec(lua_args))
    }
}
