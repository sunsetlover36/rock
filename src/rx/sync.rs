use std::time::Duration;

use mlua::{LuaSerdeExt, UserData};
use shared::components::RadialArea;
use strum::IntoEnumIterator;

use crate::{
    runtime::{
        app_data, get_app_data,
        network_replicator::protocol::{ReplicationPolicy, ReplicationTarget, SpatialFilter},
        plugins::entity::components::ComponentKey,
    },
    rx::sync::handle::PolicyHandle,
};

mod handle;

#[derive(Clone)]
pub(crate) struct RxSync {
    component_keys: mlua::Table,
    policy: ReplicationPolicy,
}
impl RxSync {
    // TODO: component keys table is being created each time
    pub fn new(lua: &mlua::Lua, target: ReplicationTarget) -> mlua::Result<Self> {
        Ok(Self {
            component_keys: Self::get_component_keys(lua)?,
            policy: ReplicationPolicy::new(target),
        })
    }

    fn get_component_keys(lua: &mlua::Lua) -> mlua::Result<mlua::Table> {
        let table = lua.create_table()?;
        for component_key in ComponentKey::iter() {
            let component_key = component_key.as_ref();
            table.set(component_key, component_key)?;
        }

        Ok(table)
    }

    fn get_fields_from_table(&self, table: mlua::Table) -> mlua::Result<Vec<String>> {
        table
            .sequence_values::<String>()
            .try_fold(Vec::new(), |mut fields, key| {
                fields.push(key?);
                Ok(fields)
            })
    }
}
impl UserData for RxSync {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("only", |_, this, arg: mlua::Value| {
            if !this.policy.hidden_fields.is_empty() {
                return Err(mlua::Error::runtime("Cannot apply both `:hide()` and `:only()` policies at the same time"));
            }

            let mut next = this.clone();
            match arg {
               mlua::Value::Table(table) => {
                   next.policy.only_fields.extend(this.get_fields_from_table(table)?);
               }
               mlua::Value::Function(func) => {
                   let table: mlua::Table = func.call(this.component_keys.clone())?;
                   next.policy.only_fields.extend(this.get_fields_from_table(table)?);
               }
               _ => {
                   return Err(mlua::Error::runtime("Failed to call `:only()`: unknown argument type, expected a table or a function"));
               }
            }

            Ok(next)
        });

        methods.add_method("hide", |_, this, arg: mlua::Value| {
            if !this.policy.only_fields.is_empty() {
                return Err(mlua::Error::runtime("Cannot apply both `:only()` and `:hide()` policies at the same time"));
            }

            let mut next = this.clone();
            match arg {
               mlua::Value::Table(table) => {
                   next.policy.hidden_fields.extend(this.get_fields_from_table(table)?);
               }
               mlua::Value::Function(func) => {
                   let table: mlua::Table = func.call(this.component_keys.clone())?;
                   next.policy.hidden_fields.extend(this.get_fields_from_table(table)?);
               }
               _ => {
                   return Err(mlua::Error::runtime("Failed to call `:hide()`: unknown argument type, expected a table or a function"));
               }
            }

            Ok(next)
        });

        methods.add_method("room", |_, this, name: String| {
            let mut next = this.clone();
            next.policy.room = Some(name);
            Ok(next)
        });

        methods.add_method("in_radius", |_, this, radius: u32| {
            match this.policy.target {
                ReplicationTarget::MemoryNode(_) => {
                    return Err(mlua::Error::runtime(
                        "Cannot apply `:in_radius()` to a memory node",
                    ));
                }
                _ => {}
            }

            let mut next = this.clone();
            next.policy.spatial = Some(SpatialFilter::Radius(radius));
            Ok(next)
        });

        methods.add_method("area", |lua, this, area: mlua::Value| {
            let area: RadialArea = lua.from_value(area)?;
            let mut next = this.clone();
            next.policy.spatial = Some(SpatialFilter::Area(area));
            Ok(next)
        });

        methods.add_method("throttle", |_, this, seconds: f64| {
            let mut next = this.clone();
            next.policy.throttle = Some(Duration::from_secs_f64(seconds));
            Ok(next)
        });

        methods.add_method("commit", |lua, this, _: ()| {
            let id = get_app_data::<app_data::NetworkReplicator>(lua)?
                .commit_policy(this.policy.clone());
            Ok(PolicyHandle::new(id))
        });
    }
}
