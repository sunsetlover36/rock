use std::hash::{Hash, Hasher};

use ahash::AHasher;
use color_eyre::eyre;

pub trait LuaResultExt {
    type Ok;
    fn wrap_err(self, msg: &str) -> eyre::Result<Self::Ok>;
}
impl<T> LuaResultExt for Result<T, mlua::Error> {
    type Ok = T;
    fn wrap_err(self, msg: &str) -> eyre::Result<T> {
        self.map_err(|e| eyre::eyre!("{}: {}", msg, e))
    }
}

pub fn get_app_data<'lua, T>(lua: &'lua mlua::Lua) -> mlua::Result<mlua::AppDataRef<'lua, T>>
where
    T: 'static,
{
    lua.app_data_ref::<T>()
        .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))
}
pub fn get_app_data_mut<'lua, T>(lua: &'lua mlua::Lua) -> mlua::Result<mlua::AppDataRefMut<'lua, T>>
where
    T: 'static,
{
    lua.app_data_mut::<T>()
        .ok_or_else(|| mlua::Error::runtime("App data is not initialized"))
}

pub fn get_str_hash(s: &str) -> u64 {
    let mut hasher = AHasher::default();
    s.hash(&mut hasher);
    hasher.finish()
}
