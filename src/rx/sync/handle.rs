use std::time::Duration;

use mlua::{LuaSerdeExt, UserData};
use shared::components::RadialArea;

use crate::runtime::{
    app_data, get_app_data, get_str_hash,
    network_replicator::{
        PolicyId,
        protocol::{PolicyFieldUpdate, ReplicationTarget, SpatialFilter},
    },
};

pub(super) struct PolicyHandle {
    id: PolicyId,
    target: ReplicationTarget,
}
impl PolicyHandle {
    pub fn new(id: PolicyId, target: ReplicationTarget) -> Self {
        Self { id, target }
    }
}
impl UserData for PolicyHandle {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("revoke", |lua, this, _: ()| {
            get_app_data::<app_data::NetworkReplicator>(lua)?.revoke_policy(this.id);
            Ok(())
        });

        methods.add_method("radius", |lua, this, radius: u32| {
            match this.target {
                ReplicationTarget::MemoryNode(_) => {
                    return Err(mlua::Error::runtime(
                        "Cannot apply `:radius()` to a memory node",
                    ));
                }
                _ => {}
            }

            get_app_data::<app_data::NetworkReplicator>(lua)?.update_policy(
                this.id,
                PolicyFieldUpdate::Spatial {
                    filter: Some(SpatialFilter::Radius(radius)),
                },
            );
            Ok(())
        });

        methods.add_method("area", |lua, this, area: mlua::Table| {
            let area: RadialArea = lua.from_value(mlua::Value::Table(area))?;
            get_app_data::<app_data::NetworkReplicator>(lua)?.update_policy(
                this.id,
                PolicyFieldUpdate::Spatial {
                    filter: Some(SpatialFilter::Area(area)),
                },
            );
            Ok(())
        });

        methods.add_method("global", |lua, this, _: ()| {
            get_app_data::<app_data::NetworkReplicator>(lua)?
                .update_policy(this.id, PolicyFieldUpdate::Spatial { filter: None });
            Ok(())
        });

        methods.add_method("room", |lua, this, name: Option<String>| {
            get_app_data::<app_data::NetworkReplicator>(lua)?.update_policy(
                this.id,
                PolicyFieldUpdate::Room {
                    id: name.map(|s| get_str_hash(&s)),
                },
            );
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
