use mlua::{LuaSerdeExt, UserData};
use rock_wire::farcaster::SendCastParams;

#[derive(Clone)]
pub(crate) struct CastRxOpcodes {
    pub send: String,
}

pub(crate) struct CastRxParams {
    pub opcodes: CastRxOpcodes,
    pub signer: String,
}

#[derive(Clone)]
pub(crate) struct CastRx {
    opcodes: CastRxOpcodes,
    signer: String,
    text: Option<String>,
    reply_hash: Option<String>,
}
impl CastRx {
    pub fn new(params: CastRxParams) -> Self {
        CastRx {
            opcodes: params.opcodes,
            signer: params.signer,
            text: None,
            reply_hash: None,
        }
    }
}

impl UserData for CastRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("text", |_, this, text: String| {
            let mut next = this.clone();
            next.text = Some(text);
            Ok(next)
        });

        methods.add_method("reply_to", |_, this, hash: String| {
            let mut next = this.clone();
            next.reply_hash = Some(hash);
            Ok(next)
        });

        methods.add_async_method("send", async |lua, this, _: ()| {
            let text = this
                .text
                .clone()
                .ok_or_else(|| mlua::Error::runtime("{}.send: cannot send an empty cast"))?;

            let table = lua.create_table()?;
            table.set("opcode", this.opcodes.send.clone())?;

            table.set(
                "args",
                lua.to_value(&SendCastParams {
                    signer_uuid: this.signer.clone(),
                    text,
                    parent: this.reply_hash.clone(),
                })?,
            )?;

            lua.yield_with::<mlua::Value>(table).await
        });
    }
}
