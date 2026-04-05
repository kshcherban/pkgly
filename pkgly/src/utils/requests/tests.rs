#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SomeThingThatTakesAnOptionString {
    #[serde(with = "crate::utils::serde_sanitize_string")]
    pub name: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KeepTrimmed {
    #[serde(with = "crate::utils::serde_sanitize_string_keep_trimmed")]
    pub name: Option<String>,
}
#[test]
pub fn test_deserialize() {
    let json = r#"{"name": "  "}"#;
    let deserialized: SomeThingThatTakesAnOptionString = serde_json::from_str(json).unwrap();
    assert_eq!(deserialized.name, None);
    let deserialized: KeepTrimmed = serde_json::from_str(json).unwrap();
    assert_eq!(deserialized.name, None);
}
#[test]
pub fn test_deserialize_null() {
    let json = r#"{"name": null}"#;
    let deserialized: SomeThingThatTakesAnOptionString = serde_json::from_str(json).unwrap();
    assert_eq!(deserialized.name, None);
    let deserialized: KeepTrimmed = serde_json::from_str(json).unwrap();
    assert_eq!(deserialized.name, None);
}

#[test]
pub fn test_serialize() {
    let thing = SomeThingThatTakesAnOptionString { name: None };
    let serialized = serde_json::to_string(&thing).unwrap();
    assert_eq!(serialized, r#"{"name":null}"#);
}

#[test]
pub fn test_serialize_some() {
    let thing = SomeThingThatTakesAnOptionString {
        name: Some("  ".to_owned()),
    };
    let serialized = serde_json::to_string(&thing).unwrap();
    assert_eq!(serialized, r#"{"name":null}"#);
}
#[test]
pub fn keeps_trimmed() {
    let json = r#"{"name": " some value "}"#;
    let deserialized: KeepTrimmed = serde_json::from_str(json).unwrap();

    assert_eq!(deserialized.name, Some("some value".to_owned()));

    let deserialized: SomeThingThatTakesAnOptionString = serde_json::from_str(json).unwrap();

    assert_eq!(deserialized.name, Some(" some value ".to_owned()));
}
