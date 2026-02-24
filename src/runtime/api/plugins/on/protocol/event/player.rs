use std::rc::Rc;

use mlua::{IntoLuaMulti, LuaSerdeExt};
use shared::{InputData, PlayerId};

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub(crate) enum PlayerEventKey {
    Connect,
    Disconnect,
    Input,
}

pub(crate) enum PlayerEventData {
    Connect {
        id: PlayerId,
    },
    Disconnect {
        id: PlayerId,
    },
    Input {
        id: PlayerId,
        name: Rc<str>,
        data: InputData,
    },
}
impl PlayerEventData {
    pub fn key(&self) -> PlayerEventKey {
        match self {
            PlayerEventData::Connect { .. } => PlayerEventKey::Connect,
            PlayerEventData::Disconnect { .. } => PlayerEventKey::Disconnect,
            PlayerEventData::Input { .. } => PlayerEventKey::Input,
        }
    }
}
impl IntoLuaMulti for PlayerEventData {
    fn into_lua_multi(self, lua: &mlua::Lua) -> mlua::Result<mlua::MultiValue> {
        match self {
            PlayerEventData::Connect { id } => id.into_lua_multi(lua),
            PlayerEventData::Disconnect { id } => id.into_lua_multi(lua),
            PlayerEventData::Input { id, name, data } => {
                let action_table = lua.create_table()?;
                action_table.set("name", name.as_ref())?;
                action_table.set("data", lua.to_value(&data)?)?;
                (id, action_table).into_lua_multi(lua)
            }
        }
    }
}
