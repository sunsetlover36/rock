use mlua::{MetaMethod, UserData};

use super::{
    protocol::{EventDescriptor, EventScope},
    rx::OnRx,
};

pub(crate) struct OnPluginLazy {
    pub scope: EventScope,
    pub descriptors: &'static [EventDescriptor],
}
impl UserData for OnPluginLazy {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |lua, this, event_name: String| {
            let descriptor = this.descriptors.iter().find(|d| d.name == event_name);
            if let Some(descriptor) = descriptor {
                let event_key = descriptor.event_key;
                let scope = this.scope;
                let factory = lua.create_function(move |_, ()| Ok(OnRx::new(event_key, scope)))?;

                return Ok(Some(factory));
            }

            Ok(None)
        });
    }
}
