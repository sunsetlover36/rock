use std::collections::{HashMap, HashSet};

use color_eyre::eyre;
use strum::IntoEnumIterator;

use crate::runtime::{LuaResultExt, plugins::entity::components::ComponentKey};

pub(crate) struct FieldRegistry {
    mapping: HashMap<String, u8>,
    reserved_fields: HashSet<String>,
    component_keys: mlua::Table,
}
impl FieldRegistry {
    fn check_bit_range(bit: u8) -> eyre::Result<()> {
        if bit >= 64 {
            return Err(eyre::eyre!(
                "ComponentKey index {} is too large for u64 mask!",
                bit
            ));
        }

        Ok(())
    }

    pub fn new(lua: &mlua::Lua) -> eyre::Result<Self> {
        let mut mapping = HashMap::new();
        let mut reserved_fields = HashSet::new();
        let component_keys = lua
            .create_table()
            .wrap_err("Failed to create `component_keys` table")?;

        for (i, key) in ComponentKey::iter().enumerate() {
            let bit_index: u8 = i.try_into()?;
            FieldRegistry::check_bit_range(bit_index)?;

            let key = key.as_ref();
            component_keys.set(key, key).wrap_err(&format!(
                "Failed to set a component key '{}' for component keys table",
                key
            ))?;

            let key = key.to_string();
            mapping.insert(key.clone(), bit_index);
            reserved_fields.insert(key);
        }

        Ok(Self {
            mapping,
            reserved_fields,
            component_keys,
        })
    }

    pub fn get_bit_index(&mut self, name: &str) -> eyre::Result<u8> {
        if let Some(&bit) = self.mapping.get(name) {
            return Ok(bit);
        }

        let new_bit: u8 = self.mapping.len().try_into()?;
        FieldRegistry::check_bit_range(new_bit)?;

        self.mapping.insert(name.to_string(), new_bit);
        Ok(new_bit)
    }

    pub fn get_component_keys(&self) -> mlua::Table {
        self.component_keys.clone()
    }

    pub fn is_reserved_field(&self, field: &str) -> bool {
        self.reserved_fields.contains(field)
    }
}
