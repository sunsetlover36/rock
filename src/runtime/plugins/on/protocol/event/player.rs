use std::rc::Rc;

use mlua::{IntoLuaMulti, LuaSerdeExt};
use rock_wire::{InputData, SignalData, SocketConnectionQuery};

use crate::runtime::{
    network_replicator::protocol::RoomId,
    plugins::player::{PlayerHandle, PlayerSnapshot},
    room_id_to_name,
};

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub(crate) enum PlayerEventKey {
    Online,
    Offline,
    Input,
    Enter,
    Exit,
    Signal,
}

pub(crate) enum PlayerEventData {
    Online {
        player: PlayerHandle,
        connection_params: SocketConnectionQuery,
    },
    Offline {
        player: PlayerSnapshot,
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
    Signal {
        player: PlayerHandle,
        signal: SignalData,
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
            PlayerEventData::Signal { .. } => PlayerEventKey::Signal,
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
            PlayerEventData::Signal { player, signal } => {
                (player, lua.to_value(&signal)?).into_lua_multi(lua)
            }
        }
    }
}
