use std::{collections::HashMap, sync::Arc};

use color_eyre::eyre;
use mlua::Lua;

pub mod scheduler;
pub use scheduler::SchedulerMessage;
use scheduler::{Scheduler, SchedulerParams};
mod plugins;
use plugins::{memory::MemoryPlugin, on::OnPlugin, scene::ScenePlugin};
pub mod protocol;
use protocol::GameModePlugin;

use crate::{
    meta_db::MetaDb,
    runtime::{app_data::GameModeAppData, utils::LuaResultExt},
};

pub struct Yielder {}
impl Yielder {
    pub fn get(lua: &Lua) -> eyre::Result<mlua::Function> {
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
    pub fn create(lua: &Lua) -> eyre::Result<mlua::RegistryKey> {
        let yielder_script = r#"
            return function(opcode)
                return function(...)
                    return coroutine.yield({ opcode = opcode, args = { ... } })
                end
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
}

pub struct ApiRegisterParams {
    pub tokio_handle: tokio::runtime::Handle,
    pub scheduler_channel_buffer: usize,
    pub meta_db: Arc<MetaDb>,
}
pub fn register(lua: &Lua, params: ApiRegisterParams) -> eyre::Result<Scheduler> {
    let (scheduler_tx, scheduler_rx) =
        flume::bounded::<SchedulerMessage>(params.scheduler_channel_buffer);

    let plugins: Vec<Box<dyn GameModePlugin>> = vec![
        Box::new(OnPlugin {}),
        Box::new(MemoryPlugin {
            meta_db: params.meta_db,
        }),
        Box::new(ScenePlugin {
            scheduler_tx: scheduler_tx.clone(),
        }),
    ];
    let mut registered_plugins = HashMap::new();

    let globals = lua.globals();
    {
        let mut app_data = lua
            .app_data_mut::<GameModeAppData>()
            .ok_or_else(|| eyre::eyre!("GameModeAppData is not initialized"))?;
        app_data.yielder = Some(Yielder::create(&lua)?);
    }

    // Global APIs
    for plugin in &plugins {
        if let Some(global_api_table) = plugin.create_global_api(&lua)? {
            let name = plugin.name().to_owned();
            let err_msg = format!("Failed to register global API for `{}` plugin", &name);
            globals.set(name, global_api_table).wrap_err(&err_msg)?;
        }
    }

    // Scene APIs
    let mut scene_plugins: HashMap<String, mlua::RegistryKey> = HashMap::new();
    for plugin in plugins {
        let name = plugin.name().to_owned();
        if let Some(scene_api_rk) = plugin.create_scene_api(&lua)? {
            scene_plugins.insert(name.clone(), scene_api_rk);
        }

        registered_plugins.insert(name, plugin);
    }

    let mut app_data = lua
        .app_data_mut::<GameModeAppData>()
        .ok_or_else(|| eyre::eyre!("GameModeAppData is not initialized"))?;
    app_data.scene_plugins = scene_plugins;

    Ok(Scheduler::new(SchedulerParams {
        plugins: registered_plugins,
        rx: scheduler_rx,
        tx: scheduler_tx,
        tokio_handle: params.tokio_handle,
    }))
}
