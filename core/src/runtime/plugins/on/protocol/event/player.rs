use std::{collections::HashMap, rc::Rc};

use mlua::{IntoLuaMulti, LuaSerdeExt};
use shared::InputData;

use crate::runtime::{
    network_replicator::protocol::RoomId, plugins::player::PlayerHandle, room_id_to_name,
};

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub(crate) enum PlayerEventKey {
    Online,
    Offline,
    Input,
    Enter,
    Exit,
    Chat,
}

pub(crate) enum PlayerEventData {
    Online {
        player: PlayerHandle,
        connection_params: HashMap<String, serde_json::Value>,
    },
    Offline {
        player: PlayerHandle,
    },
    Input {
        player: PlayerHandle,
        name: Rc<str>,
        data: InputData,
    },
    Enter {
        player: PlayerHandle,
        room: RoomId,
    },
    Exit {
        player: PlayerHandle,
        room: RoomId,
    },
    Chat {
        player: PlayerHandle,
        text: String,
    },
}
impl PlayerEventData {
    pub fn key(&self) -> PlayerEventKey {
        match self {
            PlayerEventData::Online { .. } => PlayerEventKey::Online,
            PlayerEventData::Offline { .. } => PlayerEventKey::Offline,
            PlayerEventData::Input { .. } => PlayerEventKey::Input,
            PlayerEventData::Enter { .. } => PlayerEventKey::Enter,
            PlayerEventData::Exit { .. } => PlayerEventKey::Exit,
            PlayerEventData::Chat { .. } => PlayerEventKey::Chat,
        }
    }
}
impl IntoLuaMulti for PlayerEventData {
    fn into_lua_multi(self, lua: &mlua::Lua) -> mlua::Result<mlua::MultiValue> {
        match self {
            PlayerEventData::Online {
                player,
                connection_params,
            } => (player, lua.to_value(&connection_params)?).into_lua_multi(lua),
            PlayerEventData::Offline { player } => player.into_lua_multi(lua),
            PlayerEventData::Input { player, name, data } => {
                let action_table = lua.create_table()?;
                action_table.set("name", name.as_ref())?;
                action_table.set("data", lua.to_value(&data)?)?;
                (player, action_table).into_lua_multi(lua)
            }
            PlayerEventData::Enter { player, room } => {
                (player, room_id_to_name(lua, room)?).into_lua_multi(lua)
            }
            PlayerEventData::Exit { player, room } => {
                (player, room_id_to_name(lua, room)?).into_lua_multi(lua)
            }
            PlayerEventData::Chat { player, text } => (player, text).into_lua_multi(lua),
        }
    }
}
