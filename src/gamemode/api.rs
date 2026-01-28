use std::collections::HashMap;

use color_eyre::eyre;
use mlua::{Lua, RegistryKey};

pub mod memory;
pub mod protocol;
pub mod scene;
pub mod when;
use protocol::GameModePlugin;

use crate::gamemode::{app_data::GameModeAppData, utils::LuaResultExt};

pub fn get_yielder(lua: &Lua) -> eyre::Result<mlua::Function> {
    let app_data = lua
        .app_data_ref::<GameModeAppData>()
        .ok_or_else(|| eyre::eyre!("GameModeAppData is not initialized"))?;
    let yielder_fn_rk = app_data.yielder.as_ref().ok_or_else(|| {
        eyre::eyre!("`yielder` registry key not found in app data. Did you forget to set it?")
    })?;
    let yielder_fn: mlua::Function = lua
        .registry_value(yielder_fn_rk)
        .wrap_err("`yielder` registry key not found")?;

    Ok(yielder_fn)
}

fn create_yielder(lua: &Lua) -> eyre::Result<RegistryKey> {
    let yielder_script = r#"
        return function(opcode)
            return function(...)
                return coroutine.yield({ opcode, args = { ... } })
            end
    "#;
    let yielder_fn: mlua::Function = lua
        .load(yielder_script)
        .set_name("engine/yielder")
        .eval()
        .wrap_err("Failed to create `yielder_script`")?;
    let yielder_fn_rk = lua
        .create_registry_value(yielder_fn)
        .wrap_err("Failed to store `yielder` registry value")?;

    Ok(yielder_fn_rk)
}
pub fn register(
    lua: &Lua,
    plugins: Vec<Box<dyn GameModePlugin>>,
) -> eyre::Result<HashMap<String, Box<dyn GameModePlugin>>> {
    let globals = lua.globals();
    {
        let mut app_data = lua
            .app_data_mut::<GameModeAppData>()
            .ok_or_else(|| eyre::eyre!("GameModeAppData is not initialized"))?;
        app_data.yielder = Some(create_yielder(&lua)?);
    }

    // Scene APIs
    // I register Scene APIs first, because they don't depend on anything
    let mut scene_plugins: HashMap<String, RegistryKey> = HashMap::new();
    for plugin in &plugins {
        if let Some(scene_api_rk) = plugin.create_scene_api(&lua)? {
            scene_plugins.insert(plugin.name().to_owned(), scene_api_rk);
        }
    }

    let mut app_data = lua
        .app_data_mut::<GameModeAppData>()
        .ok_or_else(|| eyre::eyre!("GameModeAppData is not initialized"))?;
    app_data.scene_plugins = scene_plugins;

    // Global APIs
    // Registered after all Scene APIs, because at least the Global API of `scene` plugin depends on Scene APIs
    let mut registered_plugins = HashMap::new();
    for plugin in plugins {
        if let Some(global_api_table) = plugin.create_global_api(&lua)? {
            let name = plugin.name().to_owned();
            let err_msg = format!("Failed to register global API for `{}` plugin", &name);
            globals
                .set(name.clone(), global_api_table)
                .wrap_err(&err_msg)?;

            registered_plugins.insert(name, plugin);
        }
    }

    Ok(registered_plugins)
}
