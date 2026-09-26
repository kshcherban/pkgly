// ABOUTME: Tests password reset link construction and trusted-origin behavior.
// ABOUTME: Ensures request headers cannot control recovery URLs.
use super::*;

#[test]
fn reset_url_encodes_token_and_preserves_panel_path() {
    let reset_url = build_reset_url("https://panel.example/pkgly/", "a+/=?&").unwrap();
    let parsed = Url::parse(&reset_url).unwrap();

    assert_eq!(parsed.path(), "/pkgly/reset-password");
    assert_eq!(
        parsed
            .query_pairs()
            .find(|(key, _)| key == "token")
            .unwrap()
            .1,
        "a+/=?&"
    );
}
