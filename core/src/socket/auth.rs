use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use shared::TicketClaims;

#[derive(Debug)]
pub(crate) enum VerifyTicketError {
    Disabled,
    Missing,
    Invalid(jsonwebtoken::errors::Error),
}

pub(crate) fn verify_ticket(token: Option<&str>) -> Result<TicketClaims, VerifyTicketError> {
    let secret = std::env::var("TICKET_SECRET").map_err(|_| VerifyTicketError::Disabled)?;
    let token = token.ok_or(VerifyTicketError::Missing)?;
    let aud = std::env::var("TICKET_AUDIENCE").unwrap_or_else(|_| "rock".to_string());

    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&[aud]);

    validation.required_spec_claims.insert("aud".to_string());
    validation.required_spec_claims.insert("exp".to_string());
    validation.required_spec_claims.insert("sub".to_string());

    let data = decode::<TicketClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(VerifyTicketError::Invalid)?;
    Ok(data.claims)
}
