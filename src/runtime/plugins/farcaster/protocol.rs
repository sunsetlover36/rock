use rock_wire::farcaster::CastIdentifierKind;
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
