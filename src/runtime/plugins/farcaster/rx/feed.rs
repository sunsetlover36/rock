use mlua::UserData;
use rock_wire::farcaster::Fid;

use crate::runtime::plugins::build_plugin_op;
use crate::rx::CursorRx;

#[derive(Clone)]
pub(crate) struct FeedRxOpcodes {
    pub for_you: String,
    pub following: String,
    pub by_filters: String,
    pub trending: String,
    pub by_channel_ids: String,
}

pub(crate) struct FeedRxParams {
    pub opcodes: FeedRxOpcodes,
    pub fid: Option<Fid>,
}

pub(crate) struct FeedRx {
    pub opcodes: FeedRxOpcodes,
    pub fid: Option<Fid>,
}
impl FeedRx {
    pub fn new(params: FeedRxParams) -> Self {
        Self {
            opcodes: params.opcodes,
            fid: params.fid,
        }
    }

    fn build_feed_op(
        &self,
        lua: &mlua::Lua,
        opcode: String,
        params: Option<mlua::Table>,
        requires_fid: bool,
    ) -> mlua::Result<mlua::Table> {
        let params = params.unwrap_or(lua.create_table()?);
        if let Some(fid) = self.fid {
            params.set("fid", fid)?;
        } else if requires_fid {
            return Err(mlua::Error::runtime(
                "feed: expected a fid; call fc.feed(fid) for this method",
            ));
        }
        build_plugin_op(lua, opcode, mlua::Value::Table(params))
    }
}
impl UserData for FeedRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("for_you", async |lua, this, params: Option<mlua::Table>| {
            let op = this.build_feed_op(&lua, this.opcodes.for_you.clone(), params, true)?;
            Ok(CursorRx::new(op))
        });

        methods.add_async_method(
            "following",
            async |lua, this, params: Option<mlua::Table>| {
                let op = this.build_feed_op(&lua, this.opcodes.following.clone(), params, true)?;
                Ok(CursorRx::new(op))
            },
        );

        methods.add_async_method("by_filters", async |lua, this, params: mlua::Table| {
            let op =
                this.build_feed_op(&lua, this.opcodes.by_filters.clone(), Some(params), false)?;
            Ok(CursorRx::new(op))
        });

        methods.add_async_method(
            "trending",
            async |lua, this, params: Option<mlua::Table>| {
                let op = this.build_feed_op(&lua, this.opcodes.trending.clone(), params, false)?;
                Ok(CursorRx::new(op))
            },
        );

        methods.add_async_method("by_channel_ids", async |lua, this, params: mlua::Table| {
            let op = this.build_feed_op(
                &lua,
                this.opcodes.by_channel_ids.clone(),
                Some(params),
                false,
            )?;
            Ok(CursorRx::new(op))
        });
    }
}
