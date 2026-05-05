use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use rock_wire::Claims;

use crate::config::TicketAuthConfig;

use super::protocol::AuthError;

pub(crate) fn verify_ticket(config: &TicketAuthConfig, token: &str) -> Result<Claims, AuthError> {
    let secret = std::env::var(&config.secret_env)
        .map_err(|err| AuthError::InternalError(err.to_string()))?;

    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&[config.audience.as_str()]);

    validation.required_spec_claims.insert("aud".to_string());
    validation.required_spec_claims.insert("exp".to_string());
    validation.required_spec_claims.insert("sub".to_string());

    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(AuthError::Invalid)?;
    Ok(data.claims)
}
