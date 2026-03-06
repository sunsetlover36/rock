use mlua::UserData;

use crate::{
    runtime::{
        GameModeClientCommand, app_data, get_app_data, network_replicator::protocol::SignalScope,
    },
    rx::RxSignal,
};

pub(super) struct BroadcastRx {}
impl UserData for BroadcastRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("message", |lua, _, text: String| {
            get_app_data::<app_data::ClientApi>(lua)?
                .send(GameModeClientCommand::Broadcast { text });

            Ok(())
        });

        methods.add_method("signal", |_, _, name: Option<String>| {
            Ok(RxSignal::new(SignalScope::Global, name))
        });
    }
}
