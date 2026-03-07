use color_eyre::eyre;
use strum::IntoEnumIterator;

use super::protocol::{AsyncTask, GameModePlugin, PluginName};

mod rx;
use rx::InputRx;

pub(crate) mod protocol;
use protocol::{ControllerButton, ControllerStick, InputSource, KeyboardKey, MouseKey};

pub struct InputPlugin {}
impl InputPlugin {
    fn create_keys_table<T>(&self, lua: &mlua::Lua) -> mlua::Result<mlua::Table>
    where
        T: IntoEnumIterator + AsRef<str> + Into<u8>,
    {
        let table = lua.create_table()?;

        for variant in T::iter() {
            let name = variant.as_ref().to_owned();
            let value: u8 = variant.into();
            table.set(name, value)?;
        }

        Ok(table)
    }
}
impl GameModePlugin for InputPlugin {
    fn name(&self) -> PluginName {
        PluginName::Input
    }

    fn create_global_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let table = lua.create_table()?;
        table.set("new", lua.create_function(|_, ()| Ok(InputRx::new()))?)?;

        let keys_table = lua.create_table()?;
        keys_table.set(
            InputSource::Keyboard.as_ref(),
            self.create_keys_table::<KeyboardKey>(lua)?,
        )?;
        keys_table.set(
            InputSource::Mouse.as_ref(),
            self.create_keys_table::<MouseKey>(lua)?,
        )?;
        keys_table.set(
            InputSource::Controller.as_ref(),
            self.create_keys_table::<ControllerButton>(lua)?,
        )?;
        keys_table.set(
            InputSource::Stick.as_ref(),
            self.create_keys_table::<ControllerStick>(lua)?,
        )?;
        table.set("bindings", keys_table)?;

        Ok(Some(table))
    }

    fn create_scene_api(&self, _: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(None)
    }
    fn handle_op(&self, _: &mlua::Lua, _: &str, _: mlua::Table) -> eyre::Result<Option<AsyncTask>> {
        Ok(None)
    }
}
