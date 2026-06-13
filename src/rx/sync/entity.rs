use mlua::{UserData, UserDataMethods};

use crate::{
    runtime::{get_app_data, get_app_data_mut, network_replicator::FieldRegistry},
    rx::sync::HasPolicy,
};

fn get_fields_mask(lua: &mlua::Lua, table: mlua::Table) -> mlua::Result<u64> {
    let mut field_registry = get_app_data_mut::<FieldRegistry>(lua)?;

    let mut mask = 0u64;
    for key in table.sequence_values::<String>() {
        let key = key?;
        let bit = match field_registry.get_bit_index(&key) {
            Some(bit) => bit,
            None => field_registry.get_or_add_bit_for(&key).map_err(|e| {
                mlua::Error::runtime(format!(
                    "Failed to add a new bit index for key '{}': {}",
                    key, e
                ))
            })?,
        };

        mask |= 1 << bit;
    }

    Ok(mask)
}

pub(crate) fn add_entity_rx_sync_methods<T, M>(methods: &mut M)
where
    T: UserData + HasPolicy + Clone + 'static,
    M: UserDataMethods<T>,
{
    methods.add_method("only", |lua, this, arg: mlua::Value| {
        let mut next = this.clone();
        match arg {
           mlua::Value::Table(table) => {
               next.policy_mut().fields_mask = get_fields_mask(lua, table)?;
           }
           mlua::Value::Function(func) => {
               let component_keys = get_app_data::<FieldRegistry>(lua)?.get_component_keys();
               let table: mlua::Table = func.call(component_keys)?;
               next.policy_mut().fields_mask = get_fields_mask(lua, table)?;
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
               next.policy_mut().fields_mask &= !get_fields_mask(lua, table)?;
           }
           mlua::Value::Function(func) => {
               let component_keys = get_app_data::<FieldRegistry>(lua)?.get_component_keys();
               let table: mlua::Table = func.call(component_keys)?;
               next.policy_mut().fields_mask &= !get_fields_mask(lua, table)?;
           }
           _ => {
               return Err(mlua::Error::runtime("Failed to call `:hide()`: unknown argument type, expected a table or a function"));
           }
        }

        Ok(next)
    });
}
