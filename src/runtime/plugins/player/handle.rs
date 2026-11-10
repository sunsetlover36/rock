use mlua::UserData;
use rock_wire::PlayerKey;

use super::{
    rx::{SignalRx, signal::SignalScope},
    vision::PlayerVision,
};
use crate::runtime::{app_data, plugins::player::presence::PlayerPresence, utils::get_app_data};

#[derive(Clone)]
pub(crate) struct PlayerHandle {
    pk: PlayerKey,
}
impl PlayerHandle {
    pub fn new(pk: PlayerKey) -> Self {
        Self { pk }
    }

    pub fn key(&self) -> PlayerKey {
        self.pk
    }

    pub fn ensure_online(lua: &mlua::Lua, pk: PlayerKey) -> mlua::Result<()> {
        if get_app_data::<app_data::ClientApi>(lua)?.0.has(pk) {
            Ok(())
        } else {
            Err(mlua::Error::runtime(format!(
                "Player {} is disconnected",
                pk.pack()
            )))
        }
    }
}
impl UserData for PlayerHandle {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("is_online", |lua, this, ()| {
            Ok(get_app_data::<app_data::ClientApi>(lua)?.0.has(this.pk))
        });
        methods.add_method("id", |_, this, _: ()| Ok(this.pk.pack()));

        methods.add_method("who", |lua, this, _: ()| {
            let api = get_app_data::<app_data::ClientApi>(lua)?;
            Ok(if api.0.has(this.pk) {
                api.0.identity(this.pk)
            } else {
                None
            })
        });
        methods.add_method("fid", |lua, this, _: ()| {
            let api = get_app_data::<app_data::ClientApi>(lua)?;
            Ok(if api.0.has(this.pk) {
                api.0.fid(this.pk)
            } else {
                None
            })
        });

        methods.add_method("kick", |lua, this, _: ()| {
            Self::ensure_online(lua, this.pk)?;
            get_app_data::<app_data::ClientApi>(lua)?.0.kick(this.pk);
            Ok(())
        });

        methods.add_method("signal", |_, this, name: Option<String>| {
            Ok(SignalRx::new(SignalScope::Player(this.pk), name))
        });

        methods.add_method("presence", |_, this, _: ()| {
            Ok(PlayerPresence::new(this.pk))
        });

        methods.add_method("vision", |_, this, _: ()| Ok(PlayerVision::new(this.pk)));
    }
}
