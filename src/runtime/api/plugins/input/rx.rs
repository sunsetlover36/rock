use mlua::UserData;
use shared::InputKind;
use strum::IntoEnumIterator;

use crate::runtime::api::plugins::input::protocol::{
    ControllerButton, ControllerStick, DefaultBinding, InputSource, KeyboardKey, Vector2DBindings,
    Vector2DControllerBindings, Vector2DKeyboardBindings,
};

#[derive(Clone)]
pub(super) struct InputRxBuilder {
    kind: Option<InputKind>,
    binding: Option<DefaultBinding>,
}
impl InputRxBuilder {
    pub fn new() -> Self {
        Self {
            kind: None,
            binding: None,
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

    fn construct_vector_bindings(&self, table: mlua::Table) -> mlua::Result<DefaultBinding> {
        let mut bindings = Vector2DBindings {
            keyboard: None,
            controller: None,
            stick: None,
        };

        for input_source in InputSource::iter() {
            if let Ok(source_map) = table.get::<mlua::Value>(input_source.as_ref()) {
                match source_map {
                    mlua::Value::Table(source_table) => match input_source {
                        InputSource::Keyboard => {
                            let up_bindings = source_table.get::<Vec<KeyboardKey>>("up")?;
                            let down_bindings = source_table.get::<Vec<KeyboardKey>>("down")?;
                            let left_bindings = source_table.get::<Vec<KeyboardKey>>("left")?;
                            let right_bindings = source_table.get::<Vec<KeyboardKey>>("right")?;

                            bindings.keyboard = Some(Vector2DKeyboardBindings {
                                up: up_bindings,
                                down: down_bindings,
                                left: left_bindings,
                                right: right_bindings,
                            });
                        }
                        InputSource::Mouse => {
                            return Err(mlua::Error::runtime(
                                "Failed to construct a Vector2D binding: cannot construct a Vector2D binding from a mouse input source",
                            ));
                        }
                        InputSource::Controller => {
                            let up_bindings = source_table.get::<Vec<ControllerButton>>("up")?;
                            let down_bindings =
                                source_table.get::<Vec<ControllerButton>>("down")?;
                            let left_bindings =
                                source_table.get::<Vec<ControllerButton>>("left")?;
                            let right_bindings =
                                source_table.get::<Vec<ControllerButton>>("right")?;

                            bindings.controller = Some(Vector2DControllerBindings {
                                up: up_bindings,
                                down: down_bindings,
                                left: left_bindings,
                                right: right_bindings,
                            });
                        }
                        InputSource::Stick => {
                            return Err(mlua::Error::runtime(
                                "Failed to construct a Vector2D binding: controller stick input key passed instead of a list of keys",
                            ));
                        }
                    },
                    mlua::Value::Integer(source_key) => {
                        let raw = u8::try_from(source_key).map_err(|_| mlua::Error::runtime("Failed to construct a Vector2D binding: input key out of range for u8"))?;
                        match input_source {
                            InputSource::Stick => {
                                let key = ControllerStick::try_from(raw).map_err(|_| {
                                    mlua::Error::runtime(
                                        "Failed to construct a Vector2D binding: key not found",
                                    )
                                })?;
                                bindings.stick = Some(key);
                            }
                            _ => {
                                return Err(mlua::Error::runtime(
                                    "Failed to construct a Vector2D binding: cannot specify a non-stick input source as a single input key",
                                ));
                            }
                        }
                    }
                    _ => {
                        return Err(mlua::Error::runtime(
                            "Failed to construct a Vector2D binding: unknown bindings schema, check the table structure",
                        ));
                    }
                }
            }
        }

        Ok(DefaultBinding::Vector2D(bindings))
    }
}
impl UserData for InputRxBuilder {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {}

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

        methods.add_method("defaults", |lua, this, table: mlua::Table| {
            let kind = this.kind.ok_or_else(|| {
                mlua::Error::runtime("Cannot set default bindings without an input kind")
            })?;

            let bindings = match kind {
                InputKind::Vector2D => this.construct_vector_bindings(table)?,
                _ => {} // InputKind::Button => this.construct_button_bindings(table),
                        // InputKind::Axis => this.construct_axis_bindings(table)
            };

            Ok(this.clone())
        });

        methods.add_method("register", |lua, this, name: String| Ok(()));
    }
}
