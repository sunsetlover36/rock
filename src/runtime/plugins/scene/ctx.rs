use mlua::UserData;

use super::protocol::{SceneYieldOp, YieldKind};

pub(crate) struct SceneCtx {}
impl UserData for SceneCtx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("sleep", async |lua, _, seconds: u64| {
            let op = lua.create_table()?;
            op.set("kind", YieldKind::Scene.as_ref())?;
            op.set("opcode", SceneYieldOp::Sleep.as_ref())?;
            op.set("args", seconds)?;

            lua.yield_with::<mlua::Value>(op).await
        });
    }
}
