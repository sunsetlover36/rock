use std::time::Duration;

use mlua::{LuaSerdeExt, UserData};
use shared::components::RadialArea;

use crate::{
    runtime::{
        app_data, get_app_data, get_app_data_mut, get_str_hash,
        network_replicator::{
            FieldRegistry,
            protocol::{PolicyRouting, ReplicationPolicy, ReplicationTarget, SpatialFilter},
        },
    },
    rx::sync::handle::PolicyHandle,
};

mod handle;

#[derive(Clone)]
pub(crate) struct RxSync {
    policy: ReplicationPolicy,
}
impl RxSync {
    pub fn new(target: ReplicationTarget) -> mlua::Result<Self> {
        Ok(Self {
            policy: ReplicationPolicy::new(target),
        })
    }

    fn get_fields_mask(&self, lua: &mlua::Lua, table: mlua::Table) -> mlua::Result<u64> {
        let mut field_registry = get_app_data_mut::<FieldRegistry>(lua)?;

        let mut mask = 0u64;
        for key in table.sequence_values::<String>() {
            let key = key?;
            let bit = field_registry.get_bit_index(&key).map_err(|e| {
                mlua::Error::runtime(format!("Failed to get a bit index of key '{}': {}", key, e))
            })?;

            mask |= 1 << bit;
        }

        Ok(mask)
    }
}
impl UserData for RxSync {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("only", |lua, this, arg: mlua::Value| {
            let mut next = this.clone();

            match arg {
               mlua::Value::Table(table) => {
                   next.policy.fields_mask = this.get_fields_mask(lua, table)?;
               }
               mlua::Value::Function(func) => {
                   let component_keys = get_app_data::<FieldRegistry>(lua)?.get_component_keys();
                   let table: mlua::Table = func.call(component_keys)?;
                   next.policy.fields_mask = this.get_fields_mask(lua, table)?;
               }
               _ => {
                   return Err(mlua::Error::runtime("Failed to call `:only()`: unknown argument type, expected a table or a function"));
               }
            }

            Ok(next)
        });

        methods.add_method("hide", |lua, this, arg: mlua::Value| {
            let mut next = this.clone();

            match arg {
               mlua::Value::Table(table) => {
                   next.policy.fields_mask &= !this.get_fields_mask(lua, table)?;
               }
               mlua::Value::Function(func) => {
                   let component_keys = get_app_data::<FieldRegistry>(lua)?.get_component_keys();
                   let table: mlua::Table = func.call(component_keys)?;
                   next.policy.fields_mask &= !this.get_fields_mask(lua, table)?;
               }
               _ => {
                   return Err(mlua::Error::runtime("Failed to call `:hide()`: unknown argument type, expected a table or a function"));
               }
            }

            Ok(next)
        });

        methods.add_method("room", |_, this, name: String| {
            let mut next = this.clone();
            let id = get_str_hash(&name);

            // Pin this room to the policy
            next.policy.routing = PolicyRouting::Pinned(id);

            Ok(next)
        });

        methods.add_method("radius", |_, this, radius: f32| {
            match this.policy.target {
                ReplicationTarget::MemoryNode(_) => {
                    return Err(mlua::Error::runtime(
                        "Cannot apply `:radius()` to a memory node",
                    ));
                }
                ReplicationTarget::Player(_) => {
                    return Err(mlua::Error::runtime(
                        "Cannot apply `:radius()` to a player session. Create an entity owned by the player instead",
                    ));
                }
                _ => {}
            }

            let mut next = this.clone();
            next.policy.spatial = SpatialFilter::Radius(radius);
            Ok(next)
        });

        methods.add_method("area", |lua, this, area: mlua::Value| {
            let area: RadialArea = lua.from_value(area)?;
            let mut next = this.clone();
            next.policy.spatial = SpatialFilter::Area(area);
            Ok(next)
        });

        methods.add_method("global", |_, this, _: ()| {
            let mut next = this.clone();
            next.policy.spatial = SpatialFilter::Global;
            Ok(next)
        });

        methods.add_method("throttle", |_, this, seconds: f64| {
            let mut next = this.clone();
            next.policy.throttle = Some(Duration::from_secs_f64(seconds));
            Ok(next)
        });

        methods.add_method("commit", |lua, this, _: ()| {
            let policy = this.policy.clone();
            let target = policy.target.clone();

            match &target {
                ReplicationTarget::MemoryNode(node) => {
                    if policy.routing == PolicyRouting::DynamicFollow {
                        return Err(mlua::Error::runtime(format!(
                            "Failed to commit a policy: memory node '{}' requires a target room",
                            node
                        )));
                    }
                }
                _ => {}
            }

            let id = get_app_data::<app_data::NetworkReplicator>(lua)?.commit_policy(policy);
            Ok(PolicyHandle::new(id, target))
        });
    }
}
