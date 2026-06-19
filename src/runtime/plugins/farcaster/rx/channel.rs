use crate::{
    runtime::plugins::{
        build_plugin_op,
        farcaster::protocol::{FollowChannelOpParams, WriteAsArgs, WriteAsOp},
        player::PlayerHandle,
        yield_op,
    },
    rx::CursorRx,
};
use mlua::{LuaSerdeExt, UserData};

#[derive(Clone)]
pub(crate) struct ChannelRxOpcodes {
    pub list: String,
    pub search: String,
    pub bulk: String,
    pub get: String,
    pub members: String,
    pub followers: String,
    pub relevant_followers: String,
    pub trending: String,
    pub follow: String,
    pub unfollow: String,
}
pub(crate) struct ChannelRxParams {
    pub opcodes: ChannelRxOpcodes,
    pub id: Option<String>,
}
pub(crate) struct ChannelRx {
    opcodes: ChannelRxOpcodes,
    id: Option<String>,
}
impl ChannelRx {
    pub fn new(p: ChannelRxParams) -> Self {
        Self {
            opcodes: p.opcodes,
            id: p.id,
        }
    }
    fn op(
        &self,
        lua: &mlua::Lua,
        code: String,
        params: Option<mlua::Table>,
        id_key: Option<&str>,
    ) -> mlua::Result<mlua::Table> {
        let params = params.unwrap_or(lua.create_table()?);
        if let Some(key) = id_key {
            params.set(
                key,
                self.id
                    .clone()
                    .ok_or_else(|| mlua::Error::runtime("channel: expected an ID"))?,
            )?;
        }
        build_plugin_op(lua, code, mlua::Value::Table(params))
    }
}
impl UserData for ChannelRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(m: &mut M) {
        macro_rules! cursor {
            ($name:literal, $field:ident, $key:expr, $required:expr) => {
                m.add_async_method($name, async |lua, this, p: Option<mlua::Table>| {
                    if $required && p.is_none() {
                        return Err(mlua::Error::runtime(concat!(
                            "channel ",
                            $name,
                            ": expected params"
                        )));
                    }
                    Ok(CursorRx::new(this.op(
                        &lua,
                        this.opcodes.$field.clone(),
                        p,
                        $key,
                    )?))
                });
            };
        }
        cursor!("list", list, None, false);
        cursor!("search", search, None, true);
        cursor!("bulk", bulk, None, true);
        cursor!("trending", trending, None, false);
        cursor!("get", get, Some("id"), false);
        cursor!("members", members, Some("channel_id"), false);
        cursor!("followers", followers, Some("id"), false);
        cursor!("relevant_followers", relevant_followers, Some("id"), true);

        m.add_async_method(
            "follow_as",
            async |lua, this, (ud, write_args): (mlua::AnyUserData, Option<WriteAsArgs>)| {
                let player = ud.borrow::<PlayerHandle>()?;
                let payload = WriteAsOp {
                    pid: player.key().pack(),
                    write_args: write_args.unwrap_or_default(),
                    params: FollowChannelOpParams {
                        channel_id: this.id.clone().ok_or_else(|| {
                            mlua::Error::runtime("channel follow: expected an ID")
                        })?,
                    },
                };
                let op =
                    build_plugin_op(&lua, this.opcodes.follow.clone(), lua.to_value(&payload)?)?;
                yield_op(&lua, "fc.channel.follow_as", op).await
            },
        );
        m.add_async_method(
            "unfollow_as",
            async |lua, this, (ud, write_args): (mlua::AnyUserData, Option<WriteAsArgs>)| {
                let player = ud.borrow::<PlayerHandle>()?;
                let payload = WriteAsOp {
                    pid: player.key().pack(),
                    write_args: write_args.unwrap_or_default(),
                    params: FollowChannelOpParams {
                        channel_id: this.id.clone().ok_or_else(|| {
                            mlua::Error::runtime("channel unfollow: expected an ID")
                        })?,
                    },
                };
                let op =
                    build_plugin_op(&lua, this.opcodes.unfollow.clone(), lua.to_value(&payload)?)?;
                yield_op(&lua, "fc.channel.unfollow_as", op).await
            },
        );
    }
}
