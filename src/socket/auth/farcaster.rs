use std::collections::HashMap;

use color_eyre::eyre;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use rock_wire::Claims;

use crate::{
    config::FarcasterAuthConfig,
    socket::auth::{
        AuthError,
        protocol::{FarcasterClaims, Jwks},
    },
};

async fn fetch_jwks(url: &str) -> eyre::Result<Jwks> {
    Ok(reqwest::get(url)
        .await?
        .error_for_status()?
        .json::<Jwks>()
        .await?)
}

#[derive(Debug, Clone)]
pub(crate) struct FarcasterVerifier {
    issuer: String,
    audience: String,
    keys: HashMap<String, DecodingKey>,
}
impl FarcasterVerifier {
    pub async fn new(config: &FarcasterAuthConfig) -> eyre::Result<Self> {
        let jwks = fetch_jwks(&config.jwks_url).await?;
        let keys = jwks
            .keys
            .into_iter()
            .filter(|jwk| jwk.kty == "RSA")
            .map(|jwk| {
                let key = DecodingKey::from_rsa_components(&jwk.n, &jwk.e)?;
                Ok((jwk.kid, key))
            })
            .collect::<Result<HashMap<_, _>, jsonwebtoken::errors::Error>>()?;

        Ok(Self {
            issuer: config.issuer.clone(),
            audience: config.audience.clone(),
            keys,
        })
    }

    pub fn verify(&self, token: &str) -> Result<Claims, AuthError> {
        let header = decode_header(token).map_err(AuthError::Invalid)?;
        if header.alg != Algorithm::RS256 {
            return Err(AuthError::Invalid(jsonwebtoken::errors::Error::from(
                jsonwebtoken::errors::ErrorKind::InvalidAlgorithm,
            )));
        }

        let invalid_token_err = || {
            AuthError::Invalid(jsonwebtoken::errors::Error::from(
                jsonwebtoken::errors::ErrorKind::InvalidToken,
            ))
        };
        let kid = header.kid.ok_or_else(invalid_token_err)?;
        let decoding_key = self.keys.get(&kid).ok_or_else(invalid_token_err)?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(&[self.audience.as_str()]);

        validation.required_spec_claims.insert("exp".to_string());
        validation.required_spec_claims.insert("iss".to_string());
        validation.required_spec_claims.insert("aud".to_string());

        let data = decode::<FarcasterClaims>(token, decoding_key, &validation)
            .map_err(AuthError::Invalid)?;
        if data.claims.sub == 0 {
            return Err(AuthError::Invalid(jsonwebtoken::errors::Error::from(
                jsonwebtoken::errors::ErrorKind::MissingRequiredClaim("sub".to_string()),
            )));
        }

        Ok(Claims {
            aud: data.claims.aud,
            exp: data.claims.exp,
            sub: format!("fc:{}", data.claims.sub),
        })
    }
}
