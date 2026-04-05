use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub enum EmailEncryption {
    #[default]
    NONE,
    StartTLS,
    TLS,
}
/// Yes the email software management software needs email settings
///
/// This is for sending reset password emails and any other emails.
#[derive(Deserialize, Serialize, Clone)]
pub struct EmailSetting {
    pub username: String,
    pub password: String,
    pub host: String,
    pub encryption: EmailEncryption,
    pub from: String,
    pub reply_to: Option<String>,
}

impl std::fmt::Debug for EmailSetting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmailSetting")
            .field("username", &self.username)
            .field("password", &"********") // Mask password
            .field("host", &self.host)
            .field("encryption", &self.encryption)
            .field("from", &self.from)
            .field("reply_to", &self.reply_to)
            .finish()
    }
}

impl Default for EmailSetting {
    fn default() -> Self {
        Self {
            username: "username".to_string(),
            password: "password".to_string(),
            host: "smtp.example.com".to_string(),
            encryption: EmailEncryption::NONE,
            from: "admin@pkgly.dev".to_owned(),
            reply_to: None,
        }
    }
}
