use mlua::UserData;

pub(super) struct TimerHandle {
    pub cancel: crate::runtime::lifetime::Cancellation,
}
impl UserData for TimerHandle {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("cancel", |_, this, _: ()| {
            this.cancel.cancel();
            Ok(())
        });
    }
}
