use mlua::UserData;

use crate::runtime::plugins::ensure_yieldable;

use super::protocol::{SceneYieldOp, YieldKind};

pub(crate) struct SceneCtx {}
impl UserData for SceneCtx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("sleep", async |lua, _, seconds: u64| {
            let op = lua.create_table()?;
            op.set("kind", YieldKind::Scene.as_ref())?;
            op.set("opcode", SceneYieldOp::Sleep.as_ref())?;
            op.set("args", seconds)?;

            ensure_yieldable(&lua, "scene ctx sleep")?;
            lua.yield_with::<mlua::Value>(op).await
        });
    }
}
