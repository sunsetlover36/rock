use std::collections::hash_map;

use mlua::{LuaSerdeExt, UserData};
use rock_wire::InputKind;

use super::protocol::{AxisBindings, ButtonBindings, InputBindings, InputEvent, Vector2DBindings};
use crate::runtime::{app_data, utils::get_app_data_mut};

#[derive(Clone)]
pub(super) struct InputRx {
    kind: InputKind,
    bindings: Option<InputBindings>,
}
impl InputRx {
    pub fn new(kind: InputKind) -> Self {
        Self {
            kind,
            bindings: None,
        }
    }
}
impl UserData for InputRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("defaults", |lua, this, table: mlua::Table| {
            match this.bindings {
                Some(_) => Err(mlua::Error::runtime("Cannot overwrite default bindings")),
                None => {
                    let table = mlua::Value::Table(table);
                    let bindings = match this.kind {
                        InputKind::Vector2D => {
                            InputBindings::Vector2D(lua.from_value::<Vector2DBindings>(table)?)
                        }
                        InputKind::Button => {
                            InputBindings::Button(lua.from_value::<ButtonBindings>(table)?)
                        }
                        InputKind::Axis => {
                            InputBindings::Axis(lua.from_value::<AxisBindings>(table)?)
                        }
                    };
                    this.bindings = Some(bindings);

                    Ok(this.clone())
                }
            }
        });

        methods.add_method("register", |lua, this, name: String| match &this.bindings {
            Some(bindings) => {
                let mut registry = get_app_data_mut::<app_data::InputEventRegistry>(lua)?;
                let next_event_id = registry.events.len();

                let entry = registry.name_to_id.entry(name.clone());
                match entry {
                    hash_map::Entry::Occupied(_) => Err(mlua::Error::runtime(format!("Failed to register an input event listener: input bindings for event `{}` already exist", name))),
                    hash_map::Entry::Vacant(entry) => {
                        entry.insert(next_event_id);
                        registry.events.push(InputEvent { name: name.into(), bindings: bindings.clone() });
                        Ok(())
                    }
                }
            },
            None => Err(mlua::Error::runtime(
                "Cannot register a new input event listener with empty bindings",
            )),
        });
    }
}
