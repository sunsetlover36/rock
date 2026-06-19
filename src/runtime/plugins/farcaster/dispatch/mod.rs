use std::sync::Arc;

use color_eyre::eyre;
use mlua::LuaSerdeExt;
use rock_wire::farcaster::Fid;
use serde::de::DeserializeOwned;

use crate::{
    clients::FarcasterApi,
    meta_db::MetaDb,
    runtime::{GameModeClientApi, LuaResultExt},
};

pub(crate) mod cast;
pub(crate) mod channel;
pub(crate) mod feed;
pub(crate) mod notifications;
pub(crate) mod signer;
pub(crate) mod user;

#[derive(Clone)]
pub(crate) struct DispatchContext {
    pub fc_api: Arc<FarcasterApi>,
    pub meta_db: Arc<MetaDb>,
    pub client_api: Arc<dyn GameModeClientApi>,
    pub default_app_fid: Option<Fid>,
    pub plugin_name: String,
}

impl DispatchContext {
    pub(crate) fn parse<T: DeserializeOwned>(
        &self,
        lua: &mlua::Lua,
        value: mlua::Value,
        domain: &str,
        method: &str,
    ) -> eyre::Result<T> {
        let message = format!(
            "{}.{}.{}: invalid parameters",
            self.plugin_name, domain, method
        );
        lua.from_value(value).wrap_err(&message)
    }

    pub(crate) fn request_error(&self, domain: &str, method: &str) -> String {
        format!(
            "{}.{}.{}: Neynar request failed",
            self.plugin_name, domain, method
        )
    }

    pub(crate) fn signer_error(&self, domain: &str, method: &str) -> String {
        format!(
            "{}.{}.{}: signer resolution failed",
            self.plugin_name, domain, method
        )
    }

    pub(crate) fn storage_error(&self, domain: &str, method: &str) -> String {
        format!(
            "{}.{}.{}: signer storage failed",
            self.plugin_name, domain, method
        )
    }
}
