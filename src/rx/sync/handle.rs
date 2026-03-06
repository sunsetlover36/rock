use std::time::Duration;

use mlua::{LuaSerdeExt, UserData};
use shared::Position;

use crate::runtime::{
    app_data, get_app_data,
    network_replicator::{PolicyId, protocol::PolicyFieldUpdate},
};

pub(super) struct PolicyHandle {
    id: PolicyId,
}
impl PolicyHandle {
    pub fn new(id: PolicyId) -> Self {
        Self { id }
    }
}
impl UserData for PolicyHandle {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("revoke", |lua, this, _: ()| {
            get_app_data::<app_data::NetworkReplicator>(lua)?.revoke_policy(this.id);
            Ok(())
        });

        methods.add_method("in_radius", |lua, this, radius: Option<u32>| {
            get_app_data::<app_data::NetworkReplicator>(lua)?
                .update_policy(this.id, PolicyFieldUpdate::Radius { radius });
            Ok(())
        });

        methods.add_method("nearest", |lua, this, position: mlua::Value| {
            let replicator = get_app_data::<app_data::NetworkReplicator>(lua)?;
            match position {
                mlua::Value::Table(table) => {
                    let position: Position = lua.from_value(mlua::Value::Table(table))?;
                    replicator.update_policy(this.id, PolicyFieldUpdate::Nearest { nearest: Some(position) });
                }
                mlua::Value::Nil => {
                    replicator.update_policy(this.id, PolicyFieldUpdate::Nearest { nearest: None });
                }
                _ => {
                    return Err(mlua::Error::runtime(format!("Failed to call `:nearest()` when trying to update a policy with ID {:?}: unknown value type", this.id)));
                }
            }

            Ok(())
        });

        methods.add_method("room", |lua, this, name: Option<String>| {
            get_app_data::<app_data::NetworkReplicator>(lua)?
                .update_policy(this.id, PolicyFieldUpdate::Room { name });
            Ok(())
        });

        methods.add_method("throttle", |lua, this, seconds: Option<f64>| {
            get_app_data::<app_data::NetworkReplicator>(lua)?.update_policy(
                this.id,
                PolicyFieldUpdate::Throttle {
                    throttle: seconds.map(|s| Duration::from_secs_f64(s)),
                },
            );
            Ok(())
        });
    }
}
