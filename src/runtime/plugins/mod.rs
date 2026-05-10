use std::collections::HashMap;

use color_eyre::eyre;
use mlua::Lua;

pub(crate) mod entity;
pub(crate) use entity::EntityPlugin;

pub(crate) mod farcaster;
pub(crate) use farcaster::FarcasterPlugin;

pub(crate) mod input;
pub(crate) use input::InputPlugin;

pub(crate) mod layer;
pub(crate) use layer::LayerPlugin;

pub(crate) mod memory;
pub(crate) use memory::MemoryPlugin;

pub(crate) mod on;
pub(crate) use on::{OnPlugin, OnPluginLazy};

pub(crate) mod player;
pub(crate) use player::PlayerPlugin;

pub(crate) mod room;
pub(crate) use room::RoomPlugin;

pub(crate) mod scene;
pub(crate) use scene::ScenePlugin;

pub(crate) mod timer;
pub(crate) use timer::TimerPlugin;

pub(crate) mod constants;
pub(crate) use constants::ConstantsPlugin;

pub(crate) mod protocol;
use protocol::*;

use crate::runtime::{
    app_data::{self},
    utils::{LuaResultExt, get_app_data},
};

pub struct Yielder {}
impl Yielder {
    pub fn get(lua: &Lua) -> mlua::Result<mlua::Function> {
        let yielder_fn = get_app_data::<app_data::Yielder>(lua)?
            .0
            .clone()
            .ok_or_else(|| mlua::Error::runtime("`yielder` function not found in app data"))?;

        Ok(yielder_fn)
    }
    pub fn create(lua: &Lua) -> eyre::Result<mlua::Function> {
        let yielder_script = r#"
            return function(opcode)
                return function(...)
                    return coroutine.yield({ opcode = opcode, args = { ... } })
                end
            end
        "#;
        let yielder_fn: mlua::Function = lua
            .load(yielder_script)
            .set_name("runtime/yielder")
            .eval()
            .wrap_err("Failed to create `yielder_script`")?;

        Ok(yielder_fn)
    }
}

pub struct PluginComposer {
    plugins: HashMap<PluginName, Box<dyn GameModePlugin>>,
}
impl PluginComposer {
    pub fn new(lua: &Lua) -> eyre::Result<Self> {
        let mut yielder = lua
            .app_data_mut::<app_data::Yielder>()
            .ok_or_else(|| eyre::eyre!("App data is not initialized"))?;
        yielder.0 = Some(Yielder::create(lua)?);

        Ok(Self {
            plugins: HashMap::new(),
        })
    }

    pub fn add_plugin(&mut self, lua: &Lua, plugin: Box<dyn GameModePlugin>) -> eyre::Result<()> {
        let plugin_name = plugin.name();
        if self.plugins.contains_key(&plugin_name) {
            return Ok(());
        }

        let globals = lua.globals();

        let err_msg = format!("Failed to initialize `{}` plugin", plugin_name);
        if let Some(global_api) = plugin.create_global_api(lua).wrap_err(&err_msg)? {
            globals
                .set(plugin_name.as_ref(), global_api)
                .wrap_err(&format!(
                    "Failed to call `add_plugin(\"{}\")`: failed to set a global table",
                    plugin_name.as_ref()
                ))?;
        };
        if let Some(scene_api) = plugin.create_scene_api(lua).wrap_err(&err_msg)? {
            let mut scene_plugins_data = lua
                .app_data_mut::<app_data::ScenePlugins>()
                .ok_or_else(|| eyre::eyre!("App data is not initialized"))?;
            let scene_plugins = &mut scene_plugins_data.0;

            if !scene_plugins.contains_key(&plugin_name) {
                scene_plugins.insert(plugin_name, scene_api);
            }

            self.plugins.insert(plugin_name, plugin);
        };

        Ok(())
    }
    pub fn remove_plugin(&mut self, lua: &Lua, name: PluginName) -> eyre::Result<()> {
        let globals = lua.globals();
        globals
            .set(name.as_ref(), mlua::Value::Nil)
            .wrap_err(&format!(
                "Failed to call `remove_plugin(\"{}\")`: cannot replace a plugin with `nil` value",
                name.as_ref()
            ))?;

        if self.plugins.contains_key(&name) {
            self.plugins.remove(&name);
            lua.app_data_mut::<app_data::ScenePlugins>()
                .ok_or_else(|| eyre::eyre!("App data is not initialized"))?
                .0
                .remove(&name);
        }

        Ok(())
    }

    pub fn consume_plugins(self) -> HashMap<PluginName, Box<dyn GameModePlugin>> {
        self.plugins
    }
}
