use std::{str::FromStr, sync::Arc};

use alloy::primitives::{Address, U256, utils::parse_units};
use color_eyre::eyre::{self, Context};
use mlua::LuaSerdeExt;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};

use crate::{
    crypto::Crypto,
    runtime::{
        LuaResultExt,
        plugins::{build_plugin_op, yield_op},
    },
};

use super::protocol::{AsyncTask, AsyncTaskResult, GameModePlugin, PluginName};

pub(crate) struct RockPlugin {
    pub crypto: Arc<Crypto>,
}

#[derive(Debug, Clone, Copy, EnumString, Display, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
enum RockOp {
    Balance,
    Holds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GetBalanceArgs {
    address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HoldsArgs {
    address: String,
    amount: String,
}

impl GameModePlugin for RockPlugin {
    fn name(&self) -> PluginName {
        PluginName::Rock
    }

    fn create_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let plugin_prefix = self.name().to_string().to_uppercase();

        let table = lua.create_table()?;

        let balance_opcode = format!("{}_{}", &plugin_prefix, RockOp::Balance.as_ref());
        let balance_fn = lua.create_async_function(move |lua, address: String| {
            let opcode = balance_opcode.clone();

            async move {
                let args = lua.to_value(&GetBalanceArgs { address })?;
                let op = build_plugin_op(&lua, opcode, args)?;
                yield_op(&lua, "rock.balance", op).await
            }
        })?;
        table.set("balance", balance_fn)?;

        let holds_opcode = format!("{}_{}", &plugin_prefix, RockOp::Holds.as_ref());
        let holds_fn =
            lua.create_async_function(move |lua, (address, amount): (String, String)| {
                let opcode = holds_opcode.clone();

                async move {
                    let args = lua.to_value(&HoldsArgs { address, amount })?;
                    let op = build_plugin_op(&lua, opcode, args)?;
                    yield_op(&lua, "rock.holds", op).await
                }
            })?;
        table.set("holds", holds_fn)?;

        Ok(Some(table))
    }
    fn handle_op(
        &self,
        lua: &mlua::Lua,
        op: &str,
        args: mlua::Value,
    ) -> eyre::Result<Option<AsyncTask>> {
        let plugin_name = self.name().to_string();
        let crypto = self.crypto.clone();

        let op = RockOp::from_str(op)?;
        match op {
            RockOp::Balance => {
                let args: GetBalanceArgs = lua
                    .from_value(args)
                    .wrap_err(&format!("{plugin_name}.balance: incorrect args"))?;

                let address: Address = args.address.parse().wrap_err_with(|| {
                    format!("{plugin_name}.holds: invalid address {}", args.address)
                })?;

                let future = Box::pin(async move {
                    let balance = crypto.rock_balance_display(address).await?;
                    Ok(AsyncTaskResult::String(balance))
                });
                Ok(Some(future))
            }

            RockOp::Holds => {
                let args: HoldsArgs = lua
                    .from_value(args)
                    .wrap_err(&format!("{plugin_name}.holds: incorrect args"))?;

                let address: Address = args.address.parse().wrap_err_with(|| {
                    format!("{plugin_name}.holds: invalid address {}", args.address)
                })?;

                let future = Box::pin(async move {
                    let decimals = crypto.rock_decimals().await?;
                    let amount: U256 = parse_units(&args.amount, decimals)
                        .wrap_err_with(|| {
                            format!("{plugin_name}.holds: invalid amount {}", args.amount)
                        })?
                        .into();

                    let balance = crypto.rock_balance(address).await?;
                    Ok(AsyncTaskResult::Bool(balance >= amount))
                });
                Ok(Some(future))
            }
        }
    }
}
