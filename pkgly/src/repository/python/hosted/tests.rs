#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use http::header::LOCATION;

#[test]
fn escapes_html_characters() {
    let escaped = html_escape("<a href=\"test\">'&'</a>");
    assert_eq!(
        escaped,
        "&lt;a href=&quot;test&quot;&gt;&#x27;&amp;&#x27;&lt;/a&gt;"
    );
}

#[test]
fn context_prefix_for_root_is_empty() {
    let path = StoragePath::default();
    let ctx = PythonSimpleRequestContext::new(&path, "/repositories/test/py-hosted/");
    assert!(ctx.prefix_to_root.is_empty());
    assert!(ctx.is_directory);
    assert!(!ctx.redirect_needed);
}

#[test]
fn context_prefix_for_simple_package() {
    let path = StoragePath::from("simple/example/");
    let ctx =
        PythonSimpleRequestContext::new(&path, "/repositories/test/py-hosted/simple/example/");
    assert_eq!(ctx.prefix_to_root, "../../");
    assert!(ctx.is_directory);
}

#[test]
fn redirect_adds_trailing_slash() {
    let response = redirect_to_trailing_slash("/repositories/test/py-hosted");
    assert_eq!(response.status(), StatusCode::MOVED_PERMANENTLY);
    let location = response.headers().get(LOCATION).unwrap();
    assert_eq!(location, "/repositories/test/py-hosted/");
}

#[test]
fn converts_base64_hash_to_hex() {
    // Test with the actual hash from the failing test
    // Base64: 0GTnr0+DaIgPWAw6gAhnsd3T2aRPyiXu9SETyZYgKj4=
    // Expected hex: d064e7af4f8368880f580c3a800867b1ddd3d9a44fca25eef52113c996202a3e
    let base64_hash = "0GTnr0+DaIgPWAw6gAhnsd3T2aRPyiXu9SETyZYgKj4=";
    let hex_hash = base64_to_hex(base64_hash);
    assert_eq!(
        hex_hash,
        Some("d064e7af4f8368880f580c3a800867b1ddd3d9a44fca25eef52113c996202a3e".to_string())
    );
}

#[test]
fn handles_invalid_base64() {
    let result = base64_to_hex("invalid!@#");
    assert_eq!(result, None);
}
