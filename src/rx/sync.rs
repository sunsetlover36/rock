use std::time::Duration;

use mlua::{LuaSerdeExt, UserData};
use shared::Position;
use strum::IntoEnumIterator;

use crate::runtime::{
    ComponentKey,
    network_replicator::protocol::{ReplicationPolicy, ReplicationTarget},
};

mod handle;

#[derive(Clone)]
pub(crate) struct RxSync {
    component_keys: mlua::Table,
    policy: ReplicationPolicy,
}
impl RxSync {
    pub fn new(lua: &mlua::Lua, target: ReplicationTarget) -> mlua::Result<Self> {
        Ok(Self {
            component_keys: Self::get_component_keys(lua)?,
            target,
            only_fields: Vec::new(),
            hidden_fields: Vec::new(),
            room: None,
            radius: None,
            nearest: None,
            throttle: None,
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
            if !this.hidden_fields.is_empty() {
                return Err(mlua::Error::runtime("Cannot apply both `:hide()` and `:only()` policies at the same time"));
            }

            let mut next = this.clone();
            match arg {
               mlua::Value::Table(table) => {
                   next.only_fields.extend(this.get_fields_from_table(table)?);
               }
               mlua::Value::Function(func) => {
                   let table: mlua::Table = func.call(this.component_keys.clone())?;
                   next.only_fields.extend(this.get_fields_from_table(table)?);
               }
               _ => {
                   return Err(mlua::Error::runtime("Failed to call `:only()`: unknown argument type, expected a table or a function"));
               }
            }

            Ok(next)
        });

        methods.add_method("hide", |_, this, arg: mlua::Value| {
            if !this.only_fields.is_empty() {
                return Err(mlua::Error::runtime("Cannot apply both `:only()` and `:hide()` policies at the same time"));
            }

            let mut next = this.clone();
            match arg {
               mlua::Value::Table(table) => {
                   next.hidden_fields.extend(this.get_fields_from_table(table)?);
               }
               mlua::Value::Function(func) => {
                   let table: mlua::Table = func.call(this.component_keys.clone())?;
                   next.hidden_fields.extend(this.get_fields_from_table(table)?);
               }
               _ => {
                   return Err(mlua::Error::runtime("Failed to call `:hide()`: unknown argument type, expected a table or a function"));
               }
            }

            Ok(next)
        });

        methods.add_method("room", |_, this, name: String| {
            let mut next = this.clone();
            next.room = Some(name);
            Ok(next)
        });

        methods.add_method("in_radius", |_, this, radius: u32| {
            match this.target {
                SyncTarget::MemoryNode(_) => {
                    return Err(mlua::Error::runtime(
                        "Cannot apply `:in_radius()` to a memory node",
                    ));
                }
                _ => {}
            }

            let mut next = this.clone();
            next.radius = Some(radius);
            Ok(next)
        });

        methods.add_method("nearest", |lua, this, position: mlua::Value| {
            let position: Position = lua.from_value(position)?;
            let mut next = this.clone();
            next.nearest = Some(position);
            Ok(next)
        });

        methods.add_method("throttle", |_, this, seconds: f64| {
            let mut next = this.clone();
            next.throttle = Some(Duration::from_secs_f64(seconds));
            Ok(next)
        });

        methods.add_method("commit", |lua, this, _: ()| {
            // f
            Ok(())
        });
    }
}
