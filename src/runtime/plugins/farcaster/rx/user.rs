use mlua::{LuaSerdeExt, UserData};
use rock_wire::farcaster::Fid;

use crate::{
    runtime::plugins::{
        farcaster::protocol::{FollowUserOpParams, WriteAsArgs, WriteAsOp},
        player::PlayerHandle,
    },
    rx::CursorRx,
};

#[derive(Clone)]
pub(crate) struct UserRxOpcodes {
    pub get_by_username: String,
    pub get_by_fids: String,
    pub search_by_username: String,
    pub get_user_casts: String,
    pub follow_user: String,
    pub unfollow_user: String,
}

pub(crate) struct UserRxParams {
    pub opcodes: UserRxOpcodes,
    pub username: Option<String>,
    pub fids: Vec<Fid>,
}

#[derive(Clone)]
pub(crate) struct UserRx {
    opcodes: UserRxOpcodes,
    username: Option<String>,
    fids: Vec<Fid>,
}
impl UserRx {
    pub fn new(params: UserRxParams) -> Self {
        Self {
            opcodes: params.opcodes,
            username: params.username,
            fids: params.fids,
        }
    }
}

impl UserData for UserRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("get", async |lua, this, _: ()| {
            let table = lua.create_table()?;
            if let Some(username) = this.username.clone() {
                table.set("opcode", this.opcodes.get_by_username.clone())?;

                let args = lua.create_table()?;
                args.set("username", username)?;
                table.set("args", args)?;
            } else {
                table.set("opcode", this.opcodes.get_by_fids.clone())?;

                let args = lua.create_table()?;
                args.set("fids", this.fids.clone())?;
                table.set("args", args)?;
            }

            lua.yield_with::<mlua::Value>(table).await
        });

        methods.add_async_method("search", async |lua, this, params: Option<mlua::Table>| {
            if let Some(username) = this.username.clone() {
                let op = lua.create_table()?;
                op.set("opcode", this.opcodes.search_by_username.clone())?;

                let args = params.unwrap_or(lua.create_table()?);
                args.set("q", username)?;
                op.set("args", args)?;

                Ok(CursorRx::new(op))
            } else {
                Err(mlua::Error::runtime("user search: expected a username"))
            }
        });

        methods.add_async_method("casts", async |lua, this, params: Option<mlua::Table>| {
            let Some(fid) = this.fids.first().copied() else {
                return Err(mlua::Error::runtime("user casts: expected a fid"));
            };

            if this.fids.len() > 1 {
                return Err(mlua::Error::runtime(
                    "user casts: expected exactly one fid, got multiple",
                ));
            }

            let op = lua.create_table()?;
            op.set("opcode", this.opcodes.get_user_casts.clone())?;

            let args = params.unwrap_or(lua.create_table()?);
            args.set("fid", fid)?;
            op.set("args", args)?;

            Ok(CursorRx::new(op))
        });

        methods.add_async_method(
            "follow_as",
            async |lua, this, (ud, write_args): (mlua::AnyUserData, Option<WriteAsArgs>)| {
                let player = ud.borrow::<PlayerHandle>()?;

                if this.fids.is_empty() {
                    return Err(mlua::Error::runtime(
                        "user follow: expected at least one target fid",
                    ));
                }

                let payload = WriteAsOp {
                    pid: player.key().pack(),
                    write_args: write_args.unwrap_or_default(),
                    params: FollowUserOpParams {
                        target_fids: this.fids.clone(),
                    },
                };

                let args = lua.to_value(&payload)?;

                let op = lua.create_table()?;
                op.set("opcode", this.opcodes.follow_user.clone())?;
                op.set("args", args)?;

                lua.yield_with::<mlua::Value>(op).await
            },
        );

        methods.add_async_method(
            "unfollow_as",
            async |lua, this, (ud, write_args): (mlua::AnyUserData, Option<WriteAsArgs>)| {
                let player = ud.borrow::<PlayerHandle>()?;

                if this.fids.is_empty() {
                    return Err(mlua::Error::runtime(
                        "user unfollow: expected at least one target fid",
                    ));
                }

                let payload = WriteAsOp {
                    pid: player.key().pack(),
                    write_args: write_args.unwrap_or_default(),
                    params: FollowUserOpParams {
                        target_fids: this.fids.clone(),
                    },
                };

                let args = lua.to_value(&payload)?;

                let op = lua.create_table()?;
                op.set("opcode", this.opcodes.unfollow_user.clone())?;
                op.set("args", args)?;

                lua.yield_with::<mlua::Value>(op).await
            },
        );
    }
}
