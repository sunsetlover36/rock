use mlua::IntoLuaMulti;

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub(crate) enum WorldEventKey {
    Awake,
}

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
