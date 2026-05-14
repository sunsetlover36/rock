use mlua::UserData;
use rock_wire::PlayerKey;

pub(crate) struct SignerRx {
    pub pk: PlayerKey,
}
impl UserData for SignerRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("get", |_, this, _: ()| {
            // f
            Ok(())
        });

        methods.add_method("status", |_, this, _: ()| {
            // f
            Ok(())
        });

        methods.add_method("request", |lua, this, _: ()| {
            // f
            Ok(())
        });
    }
}
