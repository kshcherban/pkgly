use super::{ParsedGemFileName, parse_gem_file_name};

#[test]
fn parses_plain_gem_file_name() {
    assert_eq!(
        parse_gem_file_name("rack-3.0.0.gem"),
        Some(ParsedGemFileName {
            name: "rack".to_string(),
            version: "3.0.0".to_string(),
            platform: None,
        })
    );
}

#[test]
fn parses_platform_gem_file_name_with_dashes() {
    assert_eq!(
        parse_gem_file_name("nokogiri-1.15.4-x86_64-linux.gem"),
        Some(ParsedGemFileName {
            name: "nokogiri".to_string(),
            version: "1.15.4".to_string(),
            platform: Some("x86_64-linux".to_string()),
        })
    );
}

#[test]
fn parses_name_with_dashes() {
    assert_eq!(
        parse_gem_file_name("actionpack-page_caching-1.2.3.gem"),
        Some(ParsedGemFileName {
            name: "actionpack-page_caching".to_string(),
            version: "1.2.3".to_string(),
            platform: None,
        })
    );
}

#[test]
fn rejects_non_gem_extension() {
    assert_eq!(parse_gem_file_name("rack-3.0.0.zip"), None);
}

#[test]
fn rejects_missing_version() {
    assert_eq!(parse_gem_file_name("rack.gem"), None);
    assert_eq!(parse_gem_file_name("rack-.gem"), None);
}
