use std::{str::FromStr, sync::Arc};

use color_eyre::eyre::{self, Context};
use mlua::LuaSerdeExt;
use rock_wire::farcaster::{
    BulkFetchCastsParams, Fid, GetCastConversationParams, GetCastParams, GetReactionsParams,
    GetUserByUsernameParams, GetUsersByFidsParams, SendCastParams,
};
use strum::{AsRefStr, Display, EnumString};

use crate::{
    clients::FarcasterApi,
    runtime::{
        LuaResultExt,
        plugins::{
            farcaster::rx::{UserRx, UserRxOpcodes, UserRxParams},
            player::PlayerHandle,
            protocol::AsyncTaskResult,
        },
    },
};

use super::protocol::{AsyncTask, GameModePlugin, PluginName};

mod rx;
use rx::{CastRx, CastRxOpcodes, CastRxParams, SignerRx};

mod protocol;
use protocol::CastIdentifier;

#[derive(Debug, Clone, Copy, EnumString, Display, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum FarcasterOp {
    CastGet,
    CastBulkFetch,
    CastConvo,
    CastReactions,
    CastSend,
    UserGetByUsername,
    UserGetByFids,
}

pub(crate) struct FarcasterPlugin {
    pub fc_api: Arc<FarcasterApi>,
}
impl GameModePlugin for FarcasterPlugin {
    fn name(&self) -> PluginName {
        PluginName::Farcaster
    }

    fn create_global_api(&self, _: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(None)
    }

    fn create_scene_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let plugin_name = self.name().to_string();
        let name_in_uppercase = plugin_name.to_uppercase();

        let table = lua.create_table()?;

        let cast_opcodes = CastRxOpcodes {
            get: format!("{}_{}", &name_in_uppercase, FarcasterOp::CastGet),
            bulk_fetch: format!("{}_{}", &name_in_uppercase, FarcasterOp::CastBulkFetch),
            convo: format!("{}_{}", &name_in_uppercase, FarcasterOp::CastConvo),
            reactions: format!("{}_{}", &name_in_uppercase, FarcasterOp::CastReactions),
            send: format!("{}_{}", &name_in_uppercase, FarcasterOp::CastSend),
        };

        let cast_plugin_name = plugin_name.clone();
        let cast_fn = lua.create_function(move |_, v: mlua::Value| {
            let mut ids: Vec<CastIdentifier> = Vec::new();
            match v {
                mlua::Value::String(s) => {
                    ids.push(
                        CastIdentifier::try_from(s.to_string_lossy())
                            .map_err(mlua::Error::runtime)?,
                    );
                }
                mlua::Value::Table(arr) => {
                    if arr.is_empty() {
                        return Err(mlua::Error::runtime(format!(
                            "{cast_plugin_name}.cast: cast ids list is empty"
                        )));
                    }
                    for value in arr.sequence_values::<String>() {
                        ids.push(CastIdentifier::try_from(value?).map_err(mlua::Error::runtime)?);
                    }
                }
                mlua::Value::Nil => {}
                _ => {
                    return Err(mlua::Error::runtime(format!(
                        "{cast_plugin_name}.cast: unknown argument type, expected a string, table or nil"
                    )));
                }
            }

            CastRx::new(CastRxParams {
                opcodes: cast_opcodes.clone(),
                ids,
            })
        })?;
        table.set("cast", cast_fn)?;

        let user_opcodes = UserRxOpcodes {
            get_by_username: format!("{}_{}", &name_in_uppercase, FarcasterOp::UserGetByUsername),
            get_by_fids: format!("{}_{}", &name_in_uppercase, FarcasterOp::UserGetByFids),
        };

        let user_plugin_name = plugin_name.clone();
        let user_fn = lua.create_function(move |_, v: mlua::Value| {
            let mut username: Option<String> = None;
            let mut fids: Vec<Fid> = vec![];

            match v {
                mlua::Value::String(s) => {
                    username = Some(s.to_string_lossy());
                }
                mlua::Value::Table(arr) => {
                    if arr.is_empty() {
                        return Err(mlua::Error::runtime(format!(
                            "{user_plugin_name}.user: fids list is empty"
                        )));
                    }
                    for value in arr.sequence_values::<u64>() {
                        fids.push(value?);
                    }
                }
                _ => {
                    return Err(mlua::Error::runtime(format!(
                        "{user_plugin_name}.user: unknown argument type, expected a string or a table"
                    )));
                }
            }

            Ok(UserRx::new(UserRxParams {
                opcodes: user_opcodes.clone(),
                username,
                fids,
            }))
        })?;
        table.set("user", user_fn)?;

        let signer_fn = lua.create_function(move |_, ud: mlua::AnyUserData| {
            let player = ud.borrow::<PlayerHandle>()?;
            Ok(SignerRx { pk: player.key() })
        })?;
        table.set("signer", signer_fn)?;

        Ok(Some(table))
    }
    fn handle_op(
        &self,
        lua: &mlua::Lua,
        op: &str,
        args: mlua::Table,
    ) -> eyre::Result<Option<AsyncTask>> {
        let plugin_name = self.name();
        let fc_api = self.fc_api.clone();

        let args = mlua::Value::Table(args);
        let op = FarcasterOp::from_str(op)
            .wrap_err_with(|| format!("{plugin_name}: unknown plugin op `{op}`"))?;
        match op {
            FarcasterOp::CastGet => {
                let params: GetCastParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.cast [get method]: incorrect params"
                ))?;

                let future = Box::pin(async move {
                    let cast = fc_api.get_cast(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(cast)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::CastBulkFetch => {
                let params: BulkFetchCastsParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.cast [bulk fetch method]: incorrect params"
                ))?;

                let future = Box::pin(async move {
                    let casts = fc_api.bulk_fetch_casts(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(casts)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::CastConvo => {
                let params: GetCastConversationParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.cast [convo method]: incorrect params"
                ))?;

                let future = Box::pin(async move {
                    let convo = fc_api.get_convo(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(convo)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::CastReactions => {
                let params: GetReactionsParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.cast [reactions method]: incorrect params"
                ))?;

                let future = Box::pin(async move {
                    let reactions = fc_api.get_reactions(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(reactions)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::CastSend => {
                let params: SendCastParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.cast [send method]: incorrect cast body"
                ))?;

                let future = Box::pin(async move {
                    let cast = fc_api.send_cast(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(cast)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::UserGetByUsername => {
                let params: GetUserByUsernameParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.user [get method]: incorrect params for 'GetUserByUsername'"
                ))?;

                let future = Box::pin(async move {
                    let user = fc_api.get_user_by_username(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(user)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::UserGetByFids => {
                let params: GetUsersByFidsParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.user [get method]: incorrect params for 'GetUsersByFids'"
                ))?;

                let future = Box::pin(async move {
                    let users = fc_api.get_users_by_fids(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(users)?))
                });
                Ok(Some(future))
            }
        }
    }
}
