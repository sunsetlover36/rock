use mlua::{LuaSerdeExt, UserData};
use rock_wire::farcaster::Fid;

use crate::{
    runtime::plugins::{
        build_plugin_op,
        farcaster::protocol::{FollowUserOpParams, WriteAsArgs, WriteAsOp},
        player::PlayerHandle,
        yield_op,
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
    pub followers: String,
    pub relevant_followers: String,
    pub reciprocal_followers: String,
    pub following: String,
    pub follow_suggestions: String,
    pub best_friends: String,
    pub channels: String,
    pub active_channels: String,
    pub channel_memberships: String,
    pub replies_and_recasts: String,
    pub popular_casts: String,
    pub reactions: String,
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

    fn single_fid(&self, method: &str) -> mlua::Result<Fid> {
        let Some(fid) = self.fids.first().copied() else {
            return Err(mlua::Error::runtime(format!(
                "user {method}: expected a fid"
            )));
        };

        if self.fids.len() > 1 {
            return Err(mlua::Error::runtime(format!(
                "user {method}: expected exactly one fid, got multiple"
            )));
        }

        Ok(fid)
    }
}

impl UserData for UserRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("get", async |lua, this, _: ()| {
            let (opcode, args) = if let Some(username) = this.username.clone() {
                let args = lua.create_table()?;
                args.set("username", username)?;

                (
                    this.opcodes.get_by_username.clone(),
                    mlua::Value::Table(args),
                )
            } else {
                let args = lua.create_table()?;
                args.set("fids", this.fids.clone())?;

                (this.opcodes.get_by_fids.clone(), mlua::Value::Table(args))
            };

            let op = build_plugin_op(&lua, opcode, args)?;
            yield_op(&lua, "fc.user.get", op).await
        });

        methods.add_async_method("search", async |lua, this, params: Option<mlua::Table>| {
            if let Some(username) = this.username.clone() {
                let args = params.unwrap_or(lua.create_table()?);
                args.set("q", username)?;

                let op = build_plugin_op(
                    &lua,
                    this.opcodes.search_by_username.clone(),
                    mlua::Value::Table(args),
                )?;

                Ok(CursorRx::new(op))
            } else {
                Err(mlua::Error::runtime("user search: expected a username"))
            }
        });

        methods.add_async_method("casts", async |lua, this, params: Option<mlua::Table>| {
            let fid = this.single_fid("casts")?;

            let args = params.unwrap_or(lua.create_table()?);
            args.set("fid", fid)?;

            let op = build_plugin_op(
                &lua,
                this.opcodes.get_user_casts.clone(),
                mlua::Value::Table(args),
            )?;

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
                let op = build_plugin_op(&lua, this.opcodes.follow_user.clone(), args)?;

                yield_op(&lua, "fc.user.follow_as", op).await
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
                let op = build_plugin_op(&lua, this.opcodes.unfollow_user.clone(), args)?;

                yield_op(&lua, "fc.user.unfollow_as", op).await
            },
        );

        methods.add_async_method(
            "followers",
            async |lua, this, params: Option<mlua::Table>| {
                let fid = this.single_fid("followers")?;
                let args = params.unwrap_or(lua.create_table()?);
                args.set("fid", fid)?;

                let op = build_plugin_op(
                    &lua,
                    this.opcodes.followers.clone(),
                    mlua::Value::Table(args),
                )?;
                Ok(CursorRx::new(op))
            },
        );

        methods.add_async_method(
            "relevant_followers",
            async |lua, this, params: mlua::Table| {
                let fid = this.single_fid("relevant_followers")?;
                params.set("target_fid", fid)?;

                let op = build_plugin_op(
                    &lua,
                    this.opcodes.relevant_followers.clone(),
                    mlua::Value::Table(params),
                )?;
                Ok(CursorRx::new(op))
            },
        );

        methods.add_async_method(
            "reciprocal_followers",
            async |lua, this, params: Option<mlua::Table>| {
                let fid = this.single_fid("reciprocal_followers")?;
                let args = params.unwrap_or(lua.create_table()?);
                args.set("fid", fid)?;

                let op = build_plugin_op(
                    &lua,
                    this.opcodes.reciprocal_followers.clone(),
                    mlua::Value::Table(args),
                )?;
                Ok(CursorRx::new(op))
            },
        );

        methods.add_async_method(
            "following",
            async |lua, this, params: Option<mlua::Table>| {
                let fid = this.single_fid("following")?;
                let args = params.unwrap_or(lua.create_table()?);
                args.set("fid", fid)?;

                let op = build_plugin_op(
                    &lua,
                    this.opcodes.following.clone(),
                    mlua::Value::Table(args),
                )?;
                Ok(CursorRx::new(op))
            },
        );

        methods.add_async_method(
            "suggested_follows",
            async |lua, this, params: Option<mlua::Table>| {
                let fid = this.single_fid("suggested_follows")?;
                let args = params.unwrap_or(lua.create_table()?);
                args.set("fid", fid)?;

                let op = build_plugin_op(
                    &lua,
                    this.opcodes.follow_suggestions.clone(),
                    mlua::Value::Table(args),
                )?;
                Ok(CursorRx::new(op))
            },
        );

        methods.add_async_method(
            "best_friends",
            async |lua, this, params: Option<mlua::Table>| {
                let fid = this.single_fid("best_friends")?;
                let args = params.unwrap_or(lua.create_table()?);
                args.set("fid", fid)?;

                let op = build_plugin_op(
                    &lua,
                    this.opcodes.best_friends.clone(),
                    mlua::Value::Table(args),
                )?;
                Ok(CursorRx::new(op))
            },
        );

        methods.add_async_method(
            "channels",
            async |lua, this, params: Option<mlua::Table>| {
                let args = params.unwrap_or(lua.create_table()?);
                args.set("fid", this.single_fid("channels")?)?;
                Ok(CursorRx::new(build_plugin_op(
                    &lua,
                    this.opcodes.channels.clone(),
                    mlua::Value::Table(args),
                )?))
            },
        );
        methods.add_async_method(
            "active_channels",
            async |lua, this, params: Option<mlua::Table>| {
                let args = params.unwrap_or(lua.create_table()?);
                args.set("fid", this.single_fid("active_channels")?)?;
                Ok(CursorRx::new(build_plugin_op(
                    &lua,
                    this.opcodes.active_channels.clone(),
                    mlua::Value::Table(args),
                )?))
            },
        );
        methods.add_async_method(
            "channel_memberships",
            async |lua, this, params: Option<mlua::Table>| {
                let args = params.unwrap_or(lua.create_table()?);
                args.set("fid", this.single_fid("channel_memberships")?)?;
                Ok(CursorRx::new(build_plugin_op(
                    &lua,
                    this.opcodes.channel_memberships.clone(),
                    mlua::Value::Table(args),
                )?))
            },
        );
        methods.add_async_method(
            "replies_and_recasts",
            async |lua, this, params: Option<mlua::Table>| {
                let args = params.unwrap_or(lua.create_table()?);
                args.set("fid", this.single_fid("replies_and_recasts")?)?;
                Ok(CursorRx::new(build_plugin_op(
                    &lua,
                    this.opcodes.replies_and_recasts.clone(),
                    mlua::Value::Table(args),
                )?))
            },
        );
        methods.add_async_method(
            "popular_casts",
            async |lua, this, params: Option<mlua::Table>| {
                let args = params.unwrap_or(lua.create_table()?);
                args.set("fid", this.single_fid("popular_casts")?)?;
                Ok(CursorRx::new(build_plugin_op(
                    &lua,
                    this.opcodes.popular_casts.clone(),
                    mlua::Value::Table(args),
                )?))
            },
        );
        methods.add_async_method("reactions", async |lua, this, params: mlua::Table| {
            params.set("fid", this.single_fid("reactions")?)?;
            Ok(CursorRx::new(build_plugin_op(
                &lua,
                this.opcodes.reactions.clone(),
                mlua::Value::Table(params),
            )?))
        });
    }
}
