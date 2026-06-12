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

pub(crate) mod rock;
pub(crate) use rock::RockPlugin;

pub(crate) mod scene;
pub(crate) use scene::ScenePlugin;

pub(crate) mod timer;
pub(crate) use timer::TimerPlugin;

pub(crate) mod constants;
pub(crate) use constants::ConstantsPlugin;

pub(crate) mod json;
pub(crate) use json::JsonPlugin;

pub(crate) mod protocol;
use protocol::*;

use crate::runtime::utils::LuaResultExt;

pub(crate) fn ensure_yieldable(lua: &Lua, api_name: &str) -> mlua::Result<()> {
    let coroutine: mlua::Table = lua.globals().get("coroutine")?;
    let isyieldable: mlua::Function = coroutine.get("isyieldable")?;
    let is_yieldable: bool = isyieldable.call(())?;
    if is_yieldable {
        Ok(())
    } else {
        Err(mlua::Error::runtime(format!(
            "{api_name} can only be called inside a scene coroutine"
        )))
    }
}

pub(crate) async fn yield_plugin_op(
    lua: &Lua,
    api_name: &str,
    opcode: String,
    args: mlua::Value,
) -> mlua::Result<mlua::Value> {
    ensure_yieldable(lua, api_name)?;

    let op = lua.create_table()?;
    op.set("opcode", opcode)?;
    op.set("args", args)?;

    lua.yield_with::<mlua::Value>(op).await
}

pub struct PluginComposer {
    plugins: HashMap<PluginName, Box<dyn GameModePlugin>>,
}
impl PluginComposer {
    pub fn new(_: &Lua) -> eyre::Result<Self> {
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
        if let Some(api) = plugin.create_api(lua).wrap_err(&err_msg)? {
            globals.set(plugin_name.as_ref(), api).wrap_err(&format!(
                "Failed to call `add_plugin(\"{}\")`: failed to set a global table",
                plugin_name.as_ref()
            ))?;
        };

        self.plugins.insert(plugin_name, plugin);

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
        }

        Ok(())
    }

    pub fn consume_plugins(self) -> HashMap<PluginName, Box<dyn GameModePlugin>> {
        self.plugins
    }
}
