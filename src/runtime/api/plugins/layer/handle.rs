use mlua::UserData;

use crate::runtime::api::plugins::layer::{LayerId, clear_layer_by_id};

pub(super) struct LayerHandle {
    id: LayerId,
}
impl LayerHandle {
    pub fn new(id: LayerId) -> Self {
        Self { id }
    }
}
impl UserData for LayerHandle {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("clear", |lua, this, _: ()| clear_layer_by_id(lua, this.id));
    }
}
