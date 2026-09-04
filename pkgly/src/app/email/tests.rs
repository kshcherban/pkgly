// ABOUTME: Tests backward-compatible deserialization of SMTP connection settings.
// ABOUTME: Covers optional custom ports and encryption-specific default ports.
use super::{EmailEncryption, EmailSetting};

#[test]
fn omitted_port_preserves_transport_default() {
    let setting: EmailSetting = toml::from_str(
        r#"
username = "sender"
password = "secret"
host = "smtp.example.com"
encryption = "TLS"
from = "sender@example.com"
"#,
    )
    .expect("email settings");

    assert_eq!(setting.port, None);
}

#[test]
fn explicit_port_is_preserved() {
    let setting: EmailSetting = toml::from_str(
        r#"
username = ""
password = ""
host = "mailpit"
port = 1025
encryption = "NONE"
from = "sender@example.com"
"#,
    )
    .expect("email settings");

    assert_eq!(setting.port, Some(1025));
    assert!(matches!(setting.encryption, EmailEncryption::NONE));
}
