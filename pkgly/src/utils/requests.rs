pub mod bad_request;
pub mod extensions;
pub mod ip_addr;
pub mod json;
pub fn sanitize_string(s: String) -> Option<String> {
    if s.trim().is_empty() { None } else { Some(s) }
}
pub fn sanitize_string_return_trimmed(s: String) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_owned())
    }
}
pub mod serde_sanitize_string {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Option::<String>::deserialize(deserializer)? {
            Some(s) => Ok(super::sanitize_string(s)),
            None => Ok(None),
        }
    }

    pub fn serialize<S>(s: &Option<String>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match s {
            Some(s) => {
                if s.trim().is_empty() {
                    serializer.serialize_none()
                } else {
                    serializer.serialize_some(s)
                }
            }
            None => serializer.serialize_none(),
        }
    }
}
pub mod serde_sanitize_string_keep_trimmed {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Option::<String>::deserialize(deserializer)? {
            Some(s) => Ok(super::sanitize_string_return_trimmed(s)),
            None => Ok(None),
        }
    }

    pub fn serialize<S>(s: &Option<String>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match s {
            Some(s) => {
                let s = s.trim();
                if s.is_empty() {
                    serializer.serialize_none()
                } else {
                    serializer.serialize_some(s)
                }
            }
            None => serializer.serialize_none(),
        }
    }
}
#[cfg(test)]
mod tests;
