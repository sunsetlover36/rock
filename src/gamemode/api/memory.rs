use std::rc::Rc;

use color_eyre::eyre;
use mlua::{Lua, Table};

use crate::{
    gamemode::{app_data::GameModeAppData, utils::LuaResultExt},
    meta_db::MetaDb,
};

pub fn construct(lua: &Lua, meta_db: Rc<MetaDb>) -> eyre::Result<Table> {
    let memory_table = lua
        .create_table()
        .wrap_err("Failed to create `memory_table`")?;

    let memory_peek_fn = lua
        .create_function(move |_, key: String| Ok(meta_db.get(key.as_str())))
        .wrap_err("Failed to create `memory.peek` method for `memory` namespace")?;
    memory_table
        .set("peek", memory_peek_fn)
        .wrap_err("Failed to register `memory.peek` method for `memory` namespace")?;

    let memory_table_async = lua
        .create_table()
        .wrap_err("Failed to create `memory_table_async`")?;

    let mt = lua
        .create_table()
        .wrap_err("Failed to create metatable `memory_table` for `memory_table_async` table")?;
    mt.set("__index", memory_table.clone())
        .wrap_err("Failed to set `__index` for metatable of `memory_table_async`")?;
    memory_table_async
        .set_metatable(Some(mt))
        .wrap_err("Failed to register a metatable for `memory_table_async`")?;

    let memory_recall_fn = lua
        .create_function(|_, key: String| {
            // Call `yield` with OPCODE
            println!("Memory recall: {}", key);
            Ok(())
        })
        .wrap_err("Failed to create `memory.recall` method for `memory` namespace")?;
    memory_table_async
        .set("recall", memory_recall_fn)
        .wrap_err("Failed to register `memory.recall` method for `memory` namespace")?;

    let rk = lua
        .create_registry_value(memory_table_async)
        .wrap_err("Failed to create `memory_table_async` registry value")?;
    let mut app_data = lua
        .app_data_mut::<GameModeAppData>()
        .ok_or_else(|| eyre::eyre!("GameModeAppData is not initialized"))?;
    app_data.memory_table_async = Some(rk);

    Ok(memory_table)
}
