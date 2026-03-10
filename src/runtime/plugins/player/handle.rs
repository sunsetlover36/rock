use mlua::UserData;
use shared::PlayerKey;

use super::{
    rx::{SignalRx, SyncRx},
    vision::PlayerVision,
};
use crate::runtime::{
    GameModeClientCommand, app_data, get_str_hash, network_replicator::protocol::SignalScope,
    utils::get_app_data,
};

pub(crate) struct PlayerHandle {
    pk: PlayerKey,
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

        methods.add_method("signal", |_, this, name: Option<String>| {
            Ok(SignalRx::new(SignalScope::Player(this.pk), name))
        });

        methods.add_method("sync", |_, this, _: ()| Ok(SyncRx::new(this.pk)));

        methods.add_method("room", |lua, this, name: Option<String>| {
            let id = name.map(|s| get_str_hash(&s));
            get_app_data::<app_data::NetworkReplicator>(lua)?.set_player_room(this.pk, id);
            Ok(())
        });

        methods.add_method("vision", |_, this, _: ()| Ok(PlayerVision::new(this.pk)));
    }
}
