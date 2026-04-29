use mlua::UserData;
use rock_wire::{OutgoingPacket, PlayerKey, SystemPacket};

use super::{
    rx::{SignalRx, signal::SignalScope},
    vision::PlayerVision,
};
use crate::{
    envelope::EnvelopeRecipient,
    runtime::{app_data, plugins::player::room::PlayerRoom, utils::get_app_data},
    socket::protocol::ServerMessage,
};

#[derive(Clone)]
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

        methods.add_method("who", |lua, this, _: ()| {
            Ok(get_app_data::<app_data::ClientApi>(lua)?
                .0
                .identity(this.pk))
        });

        methods.add_method("kick", |lua, this, _: ()| {
            get_app_data::<app_data::ClientApi>(lua)?
                .0
                .send(ServerMessage {
                    recipient: EnvelopeRecipient::Single(this.pk),
                    payload: OutgoingPacket::System(SystemPacket::PlayerKicked),
                });
            Ok(())
        });

        methods.add_method("signal", |_, this, name: Option<String>| {
            Ok(SignalRx::new(SignalScope::Player(this.pk), name))
        });

        methods.add_method("room", |_, this, _: ()| Ok(PlayerRoom::new(this.pk)));

        methods.add_method("vision", |_, this, _: ()| Ok(PlayerVision::new(this.pk)));
    }
}
