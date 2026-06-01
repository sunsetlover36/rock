use mlua::{LuaSerdeExt, UserData};
use rock_wire::{PlayerKey, farcaster::Fid};

use crate::runtime::{
    app_data, get_app_data,
    plugins::farcaster::protocol::{SignerRequestArgs, WithDefaultAppFid, WriteAsArgs},
};

#[derive(Clone)]
pub(crate) struct SignerRxOpcodes {
    pub request: String,
    pub get: String,
    pub refresh: String,
}

pub(crate) struct SignerRxParams {
    pub opcodes: SignerRxOpcodes,
    pub pk: PlayerKey,
    pub default_app_fid: Option<Fid>,
}

pub(crate) struct SignerRx {
    pub opcodes: SignerRxOpcodes,
    pub pk: PlayerKey,
    pub default_app_fid: Option<Fid>,
}
impl SignerRx {
    pub fn new(params: SignerRxParams) -> Self {
        Self {
            opcodes: params.opcodes,
            pk: params.pk,
            default_app_fid: params.default_app_fid,
        }
    }

    fn build_op<A>(
        &self,
        lua: &mlua::Lua,
        opcode: String,
        args: A,
        action: &str,
    ) -> mlua::Result<mlua::Table>
    where
        A: serde::Serialize,
    {
        let op = lua.create_table()?;
        op.set("opcode", opcode)?;

        let mlua::Value::Table(op_args) = lua.to_value(&args)? else {
            return Err(mlua::Error::runtime(format!(
                "signer {action}: unknown argument type, expected a table"
            )));
        };

        let fid = get_app_data::<app_data::ClientApi>(lua)?
            .0
            .fid(self.pk)
            .ok_or_else(|| {
                mlua::Error::runtime(format!(
                    "failed to {action} a signer: player fid is missing"
                ))
            })?;
        op_args.set("player_fid", fid)?;
        op.set("args", op_args)?;

        Ok(op)
    }

    fn args_or_default<A>(&self, args: Option<A>, action: &str) -> mlua::Result<A>
    where
        A: Default + WithDefaultAppFid,
    {
        let args = args.unwrap_or_default();

        let fid = self.default_app_fid.ok_or_else(|| {
            mlua::Error::runtime(format!(
                "signer {action}: missing args and no default_app_fid configured"
            ))
        })?;
        Ok(args.with_default_app_fid(fid))
    }
}

impl UserData for SignerRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method(
            "request",
            async |lua, this, args: Option<SignerRequestArgs>| {
                let args = this.args_or_default(args, "request")?;
                let op = this.build_op(&lua, this.opcodes.request.clone(), args, "request")?;
                lua.yield_with::<mlua::Value>(op).await
            },
        );

        methods.add_async_method("get", async |lua, this, args: Option<WriteAsArgs>| {
            let args = this.args_or_default(args, "get")?;
            let op = this.build_op(&lua, this.opcodes.get.clone(), args, "get")?;
            lua.yield_with::<mlua::Value>(op).await
        });

        methods.add_async_method("refresh", async |lua, this, args: Option<WriteAsArgs>| {
            let args = this.args_or_default(args, "refresh")?;
            let op = this.build_op(&lua, this.opcodes.refresh.clone(), args, "refresh")?;
            lua.yield_with::<mlua::Value>(op).await
        });
    }
}
