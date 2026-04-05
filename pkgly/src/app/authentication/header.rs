use nr_core::utils::base64_utils;
use tracing::{error, instrument};

use crate::utils::bad_request::{BadRequestErrors, InvalidAuthorizationHeader};

#[derive(Debug)]
pub enum AuthorizationHeader {
    Basic { username: String, password: String },
    Bearer { token: String },
    Session { session: String },
    Other { scheme: String, value: String },
}
impl TryFrom<String> for AuthorizationHeader {
    type Error = BadRequestErrors;
    #[instrument(skip(value), name = "AuthorizationHeader::try_from")]
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let Some(pos) = value.find(' ') else {
            if value.is_empty() {
                return Err(BadRequestErrors::InvalidAuthorizationHeader(
                    InvalidAuthorizationHeader::InvalidFormat,
                ));
            }
            return Ok(AuthorizationHeader::Bearer { token: value });
        };
        let (scheme, rest) = value.split_at(pos);
        let token = rest.trim_start();
        if token.is_empty() {
            return Err(BadRequestErrors::InvalidAuthorizationHeader(
                InvalidAuthorizationHeader::InvalidFormat,
            ));
        }
        match scheme.to_ascii_lowercase().as_str() {
            "basic" => parse_basic_header(token),
            "bearer" | "token" => Ok(AuthorizationHeader::Bearer {
                token: token.to_owned(),
            }),
            "session" => Ok(AuthorizationHeader::Session {
                session: token.to_owned(),
            }),
            _ => Ok(AuthorizationHeader::Other {
                scheme: scheme.to_owned(),
                value: token.to_owned(),
            }),
        }
    }
}
#[instrument(skip(header))]
fn parse_basic_header(header: &str) -> Result<AuthorizationHeader, BadRequestErrors> {
    let decoded = base64_utils::decode(header).map_err(|err| {
        error!("Failed to decode base64: {}", err);
        InvalidAuthorizationHeader::InvalidValue
    })?;
    let decoded = String::from_utf8(decoded).map_err(|err| {
        error!("Failed to convert bytes to string: {}", err);
        InvalidAuthorizationHeader::InvalidValue
    })?;
    let parts: Vec<&str> = decoded.split(':').collect();
    if parts.len() != 2 {
        return Err(InvalidAuthorizationHeader::InvalidBasicValue.into());
    }
    let username = parts[0];
    let password = parts[1];
    Ok(AuthorizationHeader::Basic {
        username: username.to_owned(),
        password: password.to_owned(),
    })
}

#[cfg(test)]
mod tests;
