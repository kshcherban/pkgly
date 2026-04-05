#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::AuthorizationHeader;

fn parse(header: &str) -> AuthorizationHeader {
    AuthorizationHeader::try_from(header.to_string()).expect("valid header")
}

#[test]
fn token_scheme_is_treated_as_auth_token() {
    let header = parse("Token abc123");
    match header {
        AuthorizationHeader::Bearer { token } => assert_eq!(token, "abc123"),
        other => panic!("expected bearer token, got {other:?}"),
    }
}

#[test]
fn authorization_scheme_is_case_insensitive() {
    let schemes = ["basic", "Bearer", "TOKEN", "SeSsIoN"];
    // "user:pass" in base64
    let encoded_basic = "dXNlcjpwYXNz";

    // basic
    match parse(&format!("{} {}", schemes[0], encoded_basic)) {
        AuthorizationHeader::Basic { username, password } => {
            assert_eq!(username, "user");
            assert_eq!(password, "pass");
        }
        other => panic!("expected basic header, got {other:?}"),
    }

    // bearer/token share logic
    for scheme in &schemes[1..3] {
        match parse(&format!("{scheme} super-secret")) {
            AuthorizationHeader::Bearer { token } => {
                assert_eq!(token, "super-secret");
            }
            other => panic!("expected bearer token for {scheme}, got {other:?}"),
        }
    }

    // session
    match parse(&format!("{} abcdef", schemes[3])) {
        AuthorizationHeader::Session { session } => assert_eq!(session, "abcdef"),
        other => panic!("expected session header, got {other:?}"),
    }
}

#[test]
fn bare_authorization_header_is_token() {
    let header = parse("super-secret-token");
    match header {
        AuthorizationHeader::Bearer { token } => assert_eq!(token, "super-secret-token"),
        other => panic!("expected bare token to map to bearer, got {other:?}"),
    }
}
