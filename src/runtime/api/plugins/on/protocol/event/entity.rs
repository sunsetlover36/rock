use mlua::{IntoLuaMulti, LuaSerdeExt};

use crate::runtime::api::plugins::entity::{ComponentData, ComponentKey};

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub(crate) enum EntityEventKey {
    ComponentUpdate(ComponentKey),
    CustomDataUpdate,
}

pub(crate) enum EntityEventData {
    ComponentUpdate(ComponentData),
    CustomDataUpdate(mlua::Table),
}
impl EntityEventData {
    pub fn key(&self) -> EntityEventKey {
        match self {
            EntityEventData::ComponentUpdate(data) => EntityEventKey::ComponentUpdate(data.into()),
            EntityEventData::CustomDataUpdate(_) => EntityEventKey::CustomDataUpdate,
        }
    }
}
impl IntoLuaMulti for EntityEventData {
    fn into_lua_multi(self, lua: &mlua::Lua) -> mlua::Result<mlua::MultiValue> {
        match self {
            EntityEventData::ComponentUpdate(data) => {
                Ok(mlua::MultiValue::from(vec![lua.to_value(&data)?]))
            }
            EntityEventData::CustomDataUpdate(data) => data.into_lua_multi(lua),
        }
    }
}
