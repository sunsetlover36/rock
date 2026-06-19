use crate::{
    runtime::plugins::{
        build_plugin_op,
        farcaster::protocol::{MarkNotificationsSeenOpParams, WriteAsArgs, WriteAsOp},
        player::PlayerHandle,
        yield_op,
    },
    rx::CursorRx,
};
use mlua::{LuaSerdeExt, UserData};

#[derive(Clone)]
pub(crate) struct NotificationsRxOpcodes {
    pub get: String,
    pub mark_seen: String,
}
pub(crate) struct NotificationsRx {
    opcodes: NotificationsRxOpcodes,
}
impl NotificationsRx {
    pub fn new(opcodes: NotificationsRxOpcodes) -> Self {
        Self { opcodes }
    }
}
impl UserData for NotificationsRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(m: &mut M) {
        m.add_async_method(
            "get_for",
            async |lua, this, (fid, params): (u64, Option<mlua::Table>)| {
                let args = params.unwrap_or(lua.create_table()?);
                args.set("fid", fid)?;
                Ok(CursorRx::new(build_plugin_op(
                    &lua,
                    this.opcodes.get.clone(),
                    mlua::Value::Table(args),
                )?))
            },
        );
        m.add_async_method(
            "mark_seen_as",
            async |lua,
                   this,
                   (ud, write_args, params): (
                mlua::AnyUserData,
                Option<WriteAsArgs>,
                Option<mlua::Table>,
            )| {
                let player = ud.borrow::<PlayerHandle>()?;
                let params: MarkNotificationsSeenOpParams =
                    lua.from_value(mlua::Value::Table(params.unwrap_or(lua.create_table()?)))?;
                let payload = WriteAsOp {
                    pid: player.key().pack(),
                    write_args: write_args.unwrap_or_default(),
                    params,
                };
                yield_op(
                    &lua,
                    "fc.notifications.mark_seen_as",
                    build_plugin_op(
                        &lua,
                        this.opcodes.mark_seen.clone(),
                        lua.to_value(&payload)?,
                    )?,
                )
                .await
            },
        );
    }
}
