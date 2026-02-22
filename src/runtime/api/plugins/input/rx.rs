use std::collections::hash_map;

use mlua::{LuaSerdeExt, UserData};
use shared::InputKind;

use crate::runtime::{
    api::plugins::input::protocol::{
        AxisBindings, ButtonBindings, InputBindings, Vector2DBindings,
    },
    app_data,
};

#[derive(Clone)]
pub(super) struct InputRxBuilder {
    kind: Option<InputKind>,
    bindings: Option<InputBindings>,
}
impl InputRxBuilder {
    pub fn new() -> Self {
        Self {
            kind: None,
            bindings: None,
        }
    }

    fn change_kind(&mut self, kind: InputKind) -> mlua::Result<()> {
        match self.kind {
            Some(_) => Err(mlua::Error::runtime(
                "Cannot overwrite an existing kind for the input",
            )),
            None => {
                self.kind = Some(kind);
                Ok(())
            }
        }
    }
}
impl UserData for InputRxBuilder {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("vector", |_, this, ()| {
            this.change_kind(InputKind::Vector2D)?;
            Ok(this.clone())
        });
        methods.add_method_mut("axis", |_, this, ()| {
            this.change_kind(InputKind::Axis)?;
            Ok(this.clone())
        });
        methods.add_method_mut("button", |_, this, ()| {
            this.change_kind(InputKind::Button)?;
            Ok(this.clone())
        });

        methods.add_method_mut("defaults", |lua, this, table: mlua::Table| {
            match this.bindings {
                Some(_) => Err(mlua::Error::runtime("Cannot overwrite default bindings")),
                None => {
                    let kind = this.kind.ok_or_else(|| {
                        mlua::Error::runtime("Cannot set default bindings without an input kind")
                    })?;

                    let table = mlua::Value::Table(table);
                    let bindings = match kind {
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
                let mut input_map = lua.app_data_mut::<app_data::InputMap>().ok_or_else(|| mlua::Error::runtime("App data is not initialized"))?;
                let entry = input_map.entry(name.clone());
                match entry {
                    hash_map::Entry::Occupied(_) => Err(mlua::Error::runtime(format!("Failed to register an input event listener: input bindings for event `{}` already exist", name))),
                    hash_map::Entry::Vacant(entry) => {
                        entry.insert(bindings.clone());
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
