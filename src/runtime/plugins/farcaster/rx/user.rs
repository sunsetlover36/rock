use mlua::UserData;
use rock_wire::farcaster::Fid;

#[derive(Clone)]
pub(crate) struct UserRxOpcodes {
    pub get_by_username: String,
    pub get_by_fids: String,
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
    }
}
