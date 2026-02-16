use std::{collections::HashMap, sync::Arc};

use color_eyre::eyre;
use mlua::Lua;

mod plugins;
use plugins::{
    entity::EntityPlugin,
    memory::MemoryPlugin,
    on::OnPlugin,
    scene::{SceneManagerMessage, ScenePlugin},
};
pub use plugins::{on, scene::SceneManager};
pub mod protocol;
use protocol::GameModePlugin;

use crate::{
    meta_db::MetaDb,
    runtime::{
        api::{
            on::event_descriptors::GLOBAL_EVENT_DESCRIPTORS, plugins::scene::SceneManagerParams,
        },
        app_data::GameModeAppData,
        utils::LuaResultExt,
    },
};

pub struct Yielder {}
impl Yielder {
    pub fn get(lua: &Lua) -> eyre::Result<mlua::Function> {
        let app_data = lua
            .app_data_ref::<GameModeAppData>()
            .ok_or_else(|| eyre::eyre!("App data is not initialized"))?;
        let yielder_fn = app_data
            .yielder
            .clone()
            .ok_or_else(|| eyre::eyre!("`yielder` function not found in app data"))?;

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
            .set_name("engine/yielder")
            .eval()
            .wrap_err("Failed to create `yielder_script`")?;

        Ok(yielder_fn)
    }
}

pub struct ApiRegisterParams {
    pub tokio_handle: tokio::runtime::Handle,
    pub scene_manager_channel_buffer: usize,
    pub meta_db: Arc<MetaDb>,
}
pub fn register(lua: &Lua, params: ApiRegisterParams) -> eyre::Result<SceneManager> {
    let (scene_manager_tx, scene_manager_rx) =
        flume::bounded::<SceneManagerMessage>(params.scene_manager_channel_buffer);

    let plugins: Vec<Box<dyn GameModePlugin>> = vec![
        Box::new(OnPlugin {
            descriptors: GLOBAL_EVENT_DESCRIPTORS,
        }),
        Box::new(EntityPlugin {}),
        Box::new(MemoryPlugin {
            meta_db: params.meta_db,
        }),
        Box::new(ScenePlugin {
            manager_tx: scene_manager_tx.clone(),
        }),
    ];
    let mut registered_plugins = HashMap::new();

    let globals = lua.globals();
    {
        let mut app_data = lua
            .app_data_mut::<GameModeAppData>()
            .ok_or_else(|| eyre::eyre!("App data is not initialized"))?;
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
    let mut scene_plugins: HashMap<String, mlua::Table> = HashMap::new();
    for plugin in plugins {
        let name = plugin.name().to_owned();
        if let Some(scene_api) = plugin.create_scene_api(&lua)? {
            scene_plugins.insert(name.clone(), scene_api);
        }

        registered_plugins.insert(name, plugin);
    }

    let mut app_data = lua
        .app_data_mut::<GameModeAppData>()
        .ok_or_else(|| eyre::eyre!("App data is not initialized"))?;
    app_data.scene_plugins = scene_plugins;

    Ok(SceneManager::new(SceneManagerParams {
        plugins: registered_plugins,
        rx: scene_manager_rx,
        tx: scene_manager_tx,
        tokio_handle: params.tokio_handle,
    }))
}
