use mlua::UserData;

use super::{SignalRx, signal::SignalScope};

pub(in crate::runtime::plugins::player) struct BroadcastRx {}
impl UserData for BroadcastRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("signal", |_, _, name: Option<String>| {
            Ok(SignalRx::new(SignalScope::Global, name))
        });
    }
}
