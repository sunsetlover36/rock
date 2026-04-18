use mlua::{IntoLuaMulti, LuaSerdeExt};
use shared::farcaster::WebhookEvent;

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub(crate) enum FarcasterEventKey {
    Webhook,
}

pub(crate) enum FarcasterEventData {
    Webhook(Box<WebhookEvent>),
}
impl FarcasterEventData {
    pub fn key(&self) -> FarcasterEventKey {
        match self {
            FarcasterEventData::Webhook(_) => FarcasterEventKey::Webhook,
        }
    }
}
impl IntoLuaMulti for FarcasterEventData {
    fn into_lua_multi(self, lua: &mlua::Lua) -> mlua::Result<mlua::MultiValue> {
        match self {
            FarcasterEventData::Webhook(event) => lua.to_value(&event).into_lua_multi(lua),
        }
    }
}
