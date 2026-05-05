use crate::config::{AuthConfig, AuthKind};

mod farcaster;
pub(crate) use farcaster::FarcasterVerifier;

mod ticket;
use ticket::verify_ticket;

mod protocol;
pub(crate) use protocol::AuthError;

pub(crate) fn verify_auth(
    config: &AuthConfig,
    fc_verifier: Option<&FarcasterVerifier>,
    auth: AuthKind,
    token: &str,
) -> Result<String, AuthError> {
    if config.providers.is_empty() {
        return Err(AuthError::Disabled);
    }

    if config.providers.contains(&auth) {
        match auth {
            AuthKind::Ticket => {
                let ticket_config = config
                    .ticket
                    .as_ref()
                    .ok_or(AuthError::MissingConfig(auth))?;
                let claims = verify_ticket(ticket_config, token)?;
                Ok(claims.sub)
            }
            AuthKind::Farcaster => {
                let fc_verifier = fc_verifier.ok_or(AuthError::MissingConfig(auth))?;
                let claims = fc_verifier.verify(token)?;
                Ok(claims.sub)
            }
        }
    } else {
        Err(AuthError::UnavailableProvider)
    }
}
