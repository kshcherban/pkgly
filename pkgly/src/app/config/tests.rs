// ABOUTME: Tests validation and normalization of application configuration values.
// ABOUTME: Covers trusted site URL requirements used by security-sensitive links.
use super::{ConfigError, normalize_app_url};

#[test]
fn normalizes_site_url_path() {
    assert_eq!(
        normalize_app_url("https://panel.example/pkgly").unwrap(),
        "https://panel.example/pkgly/"
    );
}

#[test]
fn rejects_untrusted_site_url_shapes() {
    for value in [
        "javascript:alert(1)",
        "https://user:pass@panel.example/",
        "https://panel.example/?redirect=evil",
        "https://panel.example/#fragment",
    ] {
        assert!(matches!(
            normalize_app_url(value),
            Err(ConfigError::InvalidAppUrl(_))
        ));
    }
}
