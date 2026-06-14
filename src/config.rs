use std::collections::HashMap;

use color_eyre::eyre;
use rock_wire::farcaster::Fid;
use serde::{Deserialize, Serialize};
use strum::EnumString;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Config {
    pub gamemode: GamemodeConfig,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub farcaster: Option<FarcasterConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub crypto: Option<CryptoConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GamemodeConfig {
    pub name: String,
}

#[derive(Debug, Clone, Copy, EnumString, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub(crate) enum AuthKind {
    Ticket,
    Farcaster,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct AuthConfig {
    pub providers: Vec<AuthKind>,

    #[serde(default)]
    pub allow_anonymous: bool,

    pub ticket: Option<TicketAuthConfig>,
    pub farcaster: Option<FarcasterAuthConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TicketAuthConfig {
    pub secret_env: String,
    pub audience: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FarcasterAuthConfig {
    pub issuer: String,
    pub audience: String,
    pub jwks_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct FarcasterConfig {
    pub webhook_env: Option<String>,
    pub api_key: Option<String>,
    pub default_app_fid: Option<Fid>,

    #[serde(default)]
    pub signers: HashMap<Fid, SignerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SignerConfig {
    pub mnemonic_env: String,
    pub derivation_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct CryptoConfig {
    pub rpc_url: Option<String>,
}

impl Config {
    pub fn filename() -> &'static str {
        "config.toml"
    }

    pub fn new() -> eyre::Result<Self> {
        let raw = std::fs::read_to_string(Config::filename())?;
        let config = toml::from_str(&raw)?;
        Ok(config)
    }
}
impl Default for Config {
    fn default() -> Self {
        Self {
            gamemode: GamemodeConfig {
                name: "grandlarc".to_owned(),
            },
            auth: Some(AuthConfig::default()),
            farcaster: Some(FarcasterConfig::default()),
            crypto: Some(CryptoConfig::default()),
        }
    }
}
