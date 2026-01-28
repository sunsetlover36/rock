use std::rc::Rc;

use color_eyre::eyre;
use mlua::{Function, Lua, RegistryKey, Table};

use crate::{
    gamemode::{api::get_yielder, api::protocol::GameModePlugin, utils::LuaResultExt},
    meta_db::MetaDb,
};

pub struct MemoryPlugin {
    pub meta_db: Rc<MetaDb>,
}
impl GameModePlugin for MemoryPlugin {
    fn name(&self) -> &str {
        "memory"
    }

    fn create_global_api(&self, lua: &Lua) -> eyre::Result<Option<Table>> {
        let memory_global_table = lua
            .create_table()
            .wrap_err("Failed to create `memory_table` for `memory` plugin")?;

        let meta_db = self.meta_db.clone();
        let peek_fn = lua
            .create_function(move |_, key: String| Ok(meta_db.get(key.as_str())))
            .wrap_err("Failed to create `peek` method for `memory` plugin")?;
        memory_global_table
            .set("peek", peek_fn)
            .wrap_err("Failed to register `peek` method for `memory` plugin")?;

        Ok(Some(memory_global_table))
    }

    fn create_scene_api(&self, lua: &Lua) -> eyre::Result<Option<RegistryKey>> {
        let memory_scene_table = lua
            .create_table()
            .wrap_err("Failed to create `memory_scene_table` for `memory` plugin")?;

        let global_memory = lua
            .globals()
            .get::<Table>(self.name())
            .wrap_err("Global table `memory` not found")?;
        let mt = lua
            .create_table()
            .wrap_err("Failed to create the metatable for `memory_scene_table`")?;
        mt.set("__index", global_memory)
            .wrap_err("Failed to set `__index` for the metatable of `memory_scene_table`")?;
        memory_scene_table
            .set_metatable(Some(mt))
            .wrap_err("Failed to register the metatable for `memory_scene_table`")?;

        let yielder_fn = get_yielder(&lua)?;
        let recall_fn = yielder_fn.call::<Function>("MEMORY_RECALL").wrap_err(
            "Failed to create `recall` method for `memory_table_async` table using yielder",
        )?;
        memory_scene_table
            .set("recall", recall_fn)
            .wrap_err("Failed to register `recall` method for `memory_scene_table` table")?;

        let fetch_fn = yielder_fn
            .call::<Function>("MEMORY_FETCH")
            .wrap_err("Failed to create `fetch` method for `memory_table_as wync` using yielder")?;
        memory_scene_table
            .set("fetch", fetch_fn)
            .wrap_err("Failed to register `fetch` method for `memory_scene_table` table")?;

        let rk = lua
            .create_registry_value(memory_scene_table)
            .wrap_err("Failed to create `memory_scene_table` registry value")?;
        Ok(Some(rk))
    }
}
