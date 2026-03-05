use mlua::{IntoLuaMulti, LuaSerdeExt};

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub(crate) enum TimerEventKey {
    Fire,
}

pub(crate) enum TimerEventData {
    Fire {
        id: String,
        data: Option<serde_json::Value>,
    },
}
impl TimerEventData {
    pub fn key(&self) -> TimerEventKey {
        match self {
            TimerEventData::Fire { .. } => TimerEventKey::Fire,
        }
    }
}
impl IntoLuaMulti for TimerEventData {
    fn into_lua_multi(self, lua: &mlua::Lua) -> mlua::Result<mlua::MultiValue> {
        match self {
            TimerEventData::Fire { id, data } => (id, lua.to_value(&data)?).into_lua_multi(lua),
        }
    }
}
