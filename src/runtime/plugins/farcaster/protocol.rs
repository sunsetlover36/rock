use mlua::{FromLua, LuaSerdeExt};
use rock_wire::{
    PlayerId, PlayerKey,
    farcaster::{CastIdentifierKind, Fid, ReactionKind, SignerResponse},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub(crate) enum CastIdentifier {
    Hash(String),
    Url(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CastIdentifierRaw {
    pub id: String,
    pub kind: CastIdentifierKind,
}

impl CastIdentifier {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Hash(s) | Self::Url(s) => s,
        }
    }

    pub fn raw(&self) -> CastIdentifierRaw {
        let id = self.as_str().to_owned();
        let kind = match self {
            Self::Hash(_) => CastIdentifierKind::Hash,
            Self::Url(_) => CastIdentifierKind::Url,
        };

        CastIdentifierRaw { id, kind }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CastIdentifierError {
    #[error("invalid cast identifier: {0}")]
    Invalid(String),
}

impl TryFrom<String> for CastIdentifier {
    type Error = CastIdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.starts_with("0x") {
            Ok(Self::Hash(value))
        } else if value.starts_with("http") {
            Ok(Self::Url(value))
        } else {
            Err(CastIdentifierError::Invalid(value))
        }
    }
}

pub(crate) trait WithDefaultAppFid {
    fn with_default_app_fid(self, default_app_fid: Fid) -> Self;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct SignerRequestArgs {
    pub app_fid: Option<Fid>,
    pub deadline: Option<u64>,
    pub redirect_url: Option<String>,
}
impl SignerRequestArgs {
    pub fn new(fid: Fid) -> Self {
        Self {
            app_fid: Some(fid),
            deadline: None,
            redirect_url: None,
        }
    }
}
impl FromLua for SignerRequestArgs {
    fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
        match value {
            mlua::Value::Integer(fid) => Ok(SignerRequestArgs::new(fid as Fid)),
            mlua::Value::Table(_) => lua.from_value(value),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "SignerRequestOptions".to_string(),
                message: Some("expected app fid integer or options table".to_string()),
            }),
        }
    }
}
impl WithDefaultAppFid for SignerRequestArgs {
    fn with_default_app_fid(mut self, default_app_fid: Fid) -> Self {
        if self.app_fid.is_none() {
            self.app_fid = Some(default_app_fid);
        }

        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SignerRequestOptions {
    pub player_fid: Fid,
    pub app_fid: Fid,
    pub deadline: Option<u64>,
    pub redirect_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredSigner {
    pub app_fid: Fid,
    pub player_fid: Fid,

    #[serde(flatten)]
    pub signer: SignerResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct WriteAsArgs {
    pub app_fid: Option<Fid>,
}
impl FromLua for WriteAsArgs {
    fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
        lua.from_value(value)
    }
}
impl WithDefaultAppFid for WriteAsArgs {
    fn with_default_app_fid(mut self, default_app_fid: Fid) -> Self {
        if self.app_fid.is_none() {
            self.app_fid = Some(default_app_fid);
        }

        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WriteAsOp<T> {
    pub pid: PlayerId,

    #[serde(flatten)]
    pub write_args: WriteAsArgs,

    #[serde(flatten)]
    pub params: T,
}
impl<T> WriteAsOp<T> {
    pub fn pk(&self) -> PlayerKey {
        PlayerKey::unpack(self.pid)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SignerGetOptions {
    pub app_fid: Fid,
    pub player_fid: Fid,
}

// -- Signed requests params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SendCastOpParams {
    pub text: String,
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PublishReactionOpParams {
    pub reaction_type: ReactionKind,
    pub target: String,
}
pub(crate) type DeleteReactionOpParams = PublishReactionOpParams;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DeleteCastOpParams {
    pub target_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FollowUserOpParams {
    pub target_fids: Vec<Fid>,
}
pub(crate) type UnfollowUserOpParams = FollowUserOpParams;
// --
