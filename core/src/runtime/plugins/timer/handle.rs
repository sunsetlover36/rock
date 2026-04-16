use mlua::UserData;

use crate::runtime::{app_data, utils::get_app_data};

pub(super) struct TimerHandle {
    pub id: String,
}
impl UserData for TimerHandle {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("cancel", |lua, this, _: ()| {
            get_app_data::<app_data::TimerManager>(lua)?
                .0
                .cancel_timer(this.id.clone());
            Ok(())
        });
    }
}
