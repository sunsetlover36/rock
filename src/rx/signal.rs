use mlua::{LuaSerdeExt, UserData};
use shared::components::RadialArea;

use crate::runtime::{
    app_data, get_app_data,
    network_replicator::protocol::{PendingSignal, SignalScope},
};

#[derive(Clone)]
pub(crate) struct RxSignal {
    scope: SignalScope,
    name: Option<String>,
    data: Option<serde_json::Map<String, serde_json::Value>>,
    area: Option<RadialArea>,
}
impl RxSignal {
    pub fn new(scope: SignalScope, name: Option<String>) -> Self {
        Self {
            scope,
            name,
            data: None,
            area: None,
        }
    }
}
impl UserData for RxSignal {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("data", |lua, this, data: mlua::Table| {
            let mut next = this.clone();
            next.data = lua.from_value(mlua::Value::Table(data))?;
            Ok(next)
        });

        methods.add_method("area", |lua, this, area: mlua::Table| {
            let area: RadialArea = lua.from_value(mlua::Value::Table(area))?;
            let mut next = this.clone();
            next.area = Some(area);
            Ok(next)
        });

        methods.add_method("send", |lua, this, _: ()| {
            let data = this.data.clone().ok_or_else(|| {
                mlua::Error::runtime("Failed to send a signal: no data to send was provided")
            })?;
            get_app_data::<app_data::NetworkReplicator>(lua)?.schedule_signal(PendingSignal {
                scope: this.scope,
                name: this.name.clone(),
                data,
                area: this.area.clone(),
            });
            Ok(())
        });
    }
}
