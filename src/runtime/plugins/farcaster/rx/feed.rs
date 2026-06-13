use mlua::UserData;
use rock_wire::farcaster::Fid;

use crate::runtime::plugins::build_plugin_op;
use crate::rx::CursorRx;

#[derive(Clone)]
pub(crate) struct FeedRxOpcodes {
    pub for_you: String,
    pub following: String,
}

pub(crate) struct FeedRxParams {
    pub opcodes: FeedRxOpcodes,
    pub fid: Fid,
}

pub(crate) struct FeedRx {
    pub opcodes: FeedRxOpcodes,
    pub fid: Fid,
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
    ) -> mlua::Result<mlua::Table> {
        let params = params.unwrap_or(lua.create_table()?);
        params.set("fid", self.fid)?;
        build_plugin_op(lua, opcode, mlua::Value::Table(params))
    }
}
impl UserData for FeedRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("for_you", async |lua, this, params: Option<mlua::Table>| {
            let op = this.build_feed_op(&lua, this.opcodes.for_you.clone(), params)?;
            Ok(CursorRx::new(op))
        });

        methods.add_async_method(
            "following",
            async |lua, this, params: Option<mlua::Table>| {
                let op = this.build_feed_op(&lua, this.opcodes.following.clone(), params)?;
                Ok(CursorRx::new(op))
            },
        );
    }
}
