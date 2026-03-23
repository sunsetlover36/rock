use mlua::UserData;

use super::protocol::GameModeEventKey;
use crate::runtime::{app_data, utils::get_app_data_mut};

pub(super) struct ListenerHandle {
    pub event_key: GameModeEventKey,
    pub name: Option<String>,
    pub seq: u64,
}
impl UserData for ListenerHandle {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("name", |_, this| Ok(this.name.clone()));
    }

    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("off", |lua, this, _: ()| {
            get_app_data_mut::<app_data::EventListeners>(lua)?
                .entry(this.event_key)
                .and_modify(|listeners| listeners.retain(|l| l.get_seq() != this.seq));

            Ok(())
        });
    }
}
