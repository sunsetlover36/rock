use mlua::UserData;

use crate::runtime::plugins::{build_scene_op, yield_op};

use super::protocol::SceneYieldOp;

pub(crate) struct SceneCtx {}
impl UserData for SceneCtx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("sleep", async |lua, _, seconds: u64| {
            let seconds = i64::try_from(seconds)
                .map_err(|_| mlua::Error::runtime("scene ctx.sleep: seconds value is too large"))?;
            let op = build_scene_op(
                &lua,
                SceneYieldOp::Sleep.as_ref(),
                mlua::Value::Integer(seconds),
            )?;
            yield_op(&lua, "scene ctx.sleep", op).await
        });
    }
}
