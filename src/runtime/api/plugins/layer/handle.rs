use mlua::UserData;

pub(super) struct LayerHandle {
    cleaners: Vec<mlua::Function>,
}
impl LayerHandle {
    pub fn new(cleaners: Vec<mlua::Function>) -> Self {
        Self { cleaners }
    }
}
impl UserData for LayerHandle {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("clear", |_, this, _: ()| {
            for cleaner in &this.cleaners {
                cleaner.call::<()>(())?;
            }

            Ok(())
        });
    }
}
