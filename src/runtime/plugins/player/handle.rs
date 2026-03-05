use mlua::UserData;
use shared::PlayerKey;

use crate::runtime::{GameModeClientCommand, app_data, utils::get_app_data};

pub(crate) struct PlayerHandle {
    pub pk: PlayerKey,
}
impl PlayerHandle {
    pub fn new(pk: PlayerKey) -> Self {
        Self { pk }
    }
}
impl UserData for PlayerHandle {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("id", |_, this, _: ()| Ok(this.pk.slot_idx));

        methods.add_method("message", |lua, this, text: String| {
            get_app_data::<app_data::ClientApi>(lua)?
                .send(GameModeClientCommand::SendMessage { pk: this.pk, text });
            Ok(())
        });

        methods.add_method("kick", |lua, this, _: ()| {
            get_app_data::<app_data::ClientApi>(lua)?
                .send(GameModeClientCommand::KickPlayer { pk: this.pk });
            Ok(())
        });
    }
}
