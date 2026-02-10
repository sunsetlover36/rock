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
