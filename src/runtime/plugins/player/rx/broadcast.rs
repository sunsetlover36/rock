use mlua::UserData;

use super::SignalRx;
use crate::runtime::{
    GameModeClientCommand, app_data, get_app_data, network_replicator::protocol::SignalScope,
};

pub(in crate::runtime::plugins::player) struct BroadcastRx {}
impl UserData for BroadcastRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("message", |lua, _, text: String| {
            get_app_data::<app_data::ClientApi>(lua)?
                .send(GameModeClientCommand::Broadcast { text });

            Ok(())
        });

        methods.add_method("signal", |_, _, name: Option<String>| {
            Ok(SignalRx::new(SignalScope::Global, name))
        });
    }
}
