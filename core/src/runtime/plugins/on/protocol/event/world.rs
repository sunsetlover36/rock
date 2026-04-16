use mlua::IntoLuaMulti;

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub(crate) enum WorldEventKey {
    Awake,
    Impromptu,
}

pub(crate) enum WorldEventData {
    Awake,
    Impromptu { name: Option<String> },
}
impl WorldEventData {
    pub fn key(&self) -> WorldEventKey {
        match self {
            WorldEventData::Awake => WorldEventKey::Awake,
            WorldEventData::Impromptu { .. } => WorldEventKey::Impromptu,
        }
    }
}
impl IntoLuaMulti for WorldEventData {
    fn into_lua_multi(self, lua: &mlua::Lua) -> mlua::Result<mlua::MultiValue> {
        match self {
            WorldEventData::Awake => ().into_lua_multi(lua),
            WorldEventData::Impromptu { name } => name.into_lua_multi(lua),
        }
    }
}
