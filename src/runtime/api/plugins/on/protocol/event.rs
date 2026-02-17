use mlua::IntoLuaMulti;

pub(crate) mod world;
pub(crate) use world::*;

pub(crate) mod player;
pub(crate) use player::*;

pub(crate) mod entity;
pub(crate) use entity::*;

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

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub(crate) enum GameModeEventKey {
    World(WorldEventKey),
    Player(PlayerEventKey),
    Entity(EntityEventKey),
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
