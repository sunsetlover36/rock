use mlua::{IntoLuaMulti, LuaSerdeExt};
use shared::InputAction;

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub(crate) enum PlayerEventKey {
    Connect,
    Disconnect,
    Input,
}

pub(crate) enum PlayerEventData {
    Connect { id: u64 },
    Disconnect { id: u64 },
    Input(InputAction),
}
impl PlayerEventData {
    pub fn key(&self) -> PlayerEventKey {
        match self {
            PlayerEventData::Connect { .. } => PlayerEventKey::Connect,
            PlayerEventData::Disconnect { .. } => PlayerEventKey::Disconnect,
            PlayerEventData::Input(_) => PlayerEventKey::Input,
        }
    }
}
impl IntoLuaMulti for PlayerEventData {
    fn into_lua_multi(self, lua: &mlua::Lua) -> mlua::Result<mlua::MultiValue> {
        match self {
            PlayerEventData::Connect { id } => id.into_lua_multi(lua),
            PlayerEventData::Disconnect { id } => id.into_lua_multi(lua),
            PlayerEventData::Input(action) => lua.to_value(&action)?.into_lua_multi(lua),
        }
    }
}
