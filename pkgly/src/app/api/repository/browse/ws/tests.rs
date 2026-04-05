#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::{WebsocketOutgoingMessage, encode_outgoing_message};

#[test]
fn encode_outgoing_message_serializes_simple_variant() {
    let payload = encode_outgoing_message(&WebsocketOutgoingMessage::EndOfDirectory);
    assert!(payload.contains("EndOfDirectory"));
}
