#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::parse_mailbox;

#[test]
fn parse_mailbox_accepts_valid_addresses() {
    assert!(parse_mailbox("user@example.com", "from").is_ok());
}

#[test]
fn parse_mailbox_rejects_invalid_addresses() {
    assert!(parse_mailbox("invalid-address", "from").is_err());
}
