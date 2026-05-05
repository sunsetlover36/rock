use serde::Deserialize;

use crate::config::AuthKind;

#[derive(Debug, Clone)]
pub(crate) enum AuthError {
    Disabled,
    MissingAuthKind,
    MissingToken,
    MissingConfig(AuthKind),
    UnavailableProvider,
    JwksUnavailable(String),
    Invalid(jsonwebtoken::errors::Error),
    InternalError(String),
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Jwks {
    pub keys: Vec<Jwk>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Jwk {
    pub kty: String,
    pub kid: String,
    pub n: String,
    pub e: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FarcasterClaims {
    pub sub: u64,
    pub exp: usize,
    pub iss: String,
    pub aud: String,
}
