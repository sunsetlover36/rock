use color_eyre::eyre;
use strum::IntoEnumIterator;

use crate::runtime::{
    network_replicator::protocol::AreaShape,
    plugins::input::protocol::{
        ControllerButton, ControllerStick, InputSource, KeyboardKey, MouseKey,
    },
};

use super::protocol::{GameModePlugin, PluginName};

pub(crate) struct ConstantsPlugin {}
impl ConstantsPlugin {
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
impl GameModePlugin for ConstantsPlugin {
    fn name(&self) -> PluginName {
        PluginName::Constants
    }

    fn create_global_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let table = lua.create_table()?;

        let area_shape = lua.create_table()?;
        for shape in AreaShape::iter() {
            let shape = shape.as_ref();
            area_shape.set(shape, shape)?;
        }
        table.set("AreaShape", area_shape)?;

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
        table.set("Input", keys_table)?;

        Ok(Some(table))
    }

    fn create_scene_api(&self, _: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(None)
    }
    fn handle_op(
        &self,
        _: &mlua::Lua,
        _: &str,
        _: mlua::Table,
    ) -> eyre::Result<Option<super::protocol::AsyncTask>> {
        Ok(None)
    }
}
