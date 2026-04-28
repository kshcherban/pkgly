use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use nr_api::{Client, ClientConfig, CreateRepositoryRequest};
use serde_json::json;

fn serve_once(response: String) -> Result<(String, thread::JoinHandle<String>), std::io::Error> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let handle = thread::spawn(move || {
        let (mut stream, _) = match listener.accept() {
            Ok(value) => value,
            Err(err) => return format!("accept error: {err}"),
        };
        let mut buffer = [0_u8; 8192];
        let read = match stream.read(&mut buffer) {
            Ok(value) => value,
            Err(err) => return format!("read error: {err}"),
        };
        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
        let _ = stream.write_all(response.as_bytes());
        request
    });
    Ok((format!("http://{addr}"), handle))
}

fn json_response(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{body}",
        body.len()
    )
}

fn text_response(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

fn join_request(handle: thread::JoinHandle<String>) -> String {
    match handle.join() {
        Ok(value) => value,
        Err(_) => panic!("mock server thread panicked"),
    }
}

#[tokio::test]
async fn client_joins_urls_and_injects_bearer_auth() {
    let response = json_response("200 OK", "[]");
    let (base_url, handle) =
        serve_once(response).unwrap_or_else(|err| panic!("server failed: {err}"));
    let client = Client::new(ClientConfig {
        base_url,
        token: Some("secret-token".to_string()),
        user_agent: Some("pkglyctl-test".to_string()),
    })
    .unwrap_or_else(|err| panic!("client failed: {err}"));

    client
        .list_repositories()
        .await
        .unwrap_or_else(|err| panic!("request failed: {err}"));

    let request = join_request(handle);
    assert!(request.starts_with("GET /api/repository/list HTTP/1.1"));
    assert!(request.contains("authorization: Bearer secret-token"));
}

#[tokio::test]
async fn http_errors_preserve_response_body() {
    let response = text_response("409 Conflict", "name conflict");
    let (base_url, handle) =
        serve_once(response).unwrap_or_else(|err| panic!("server failed: {err}"));
    let client = Client::new(ClientConfig {
        base_url,
        token: None,
        user_agent: None,
    })
    .unwrap_or_else(|err| panic!("client failed: {err}"));

    let err = client
        .list_repositories()
        .await
        .err()
        .unwrap_or_else(|| panic!("expected http error"));
    assert!(err.to_string().contains("409 Conflict"));
    assert!(err.to_string().contains("name conflict"));
    let request = join_request(handle);
    assert!(request.starts_with("GET /api/repository/list HTTP/1.1"));
}

#[tokio::test]
async fn create_repository_posts_expected_json_body() {
    let body = "{\"id\":\"00000000-0000-0000-0000-000000000001\",\"storage_id\":\"00000000-0000-0000-0000-000000000002\",\"storage_name\":\"local\",\"name\":\"libs\",\"repository_type\":\"maven\",\"visibility\":\"Public\",\"active\":true,\"updated_at\":\"2026-04-28T00:00:00+00:00\",\"created_at\":\"2026-04-28T00:00:00+00:00\"}";
    let response = json_response("201 Created", body);
    let (base_url, handle) =
        serve_once(response).unwrap_or_else(|err| panic!("server failed: {err}"));
    let client = Client::new(ClientConfig {
        base_url,
        token: None,
        user_agent: None,
    })
    .unwrap_or_else(|err| panic!("client failed: {err}"));

    let request = CreateRepositoryRequest {
        name: "libs".to_string(),
        storage: None,
        storage_name: Some("local".to_string()),
        configs: Default::default(),
    };
    client
        .create_repository("maven", &request)
        .await
        .unwrap_or_else(|err| panic!("request failed: {err}"));

    let request = join_request(handle);
    assert!(request.starts_with("POST /api/repository/new/maven HTTP/1.1"));
    assert!(request.contains("\"name\":\"libs\""));
    assert!(request.contains("\"storage_name\":\"local\""));
}

#[tokio::test]
async fn maven_upload_uses_repository_route_put() {
    let response = "HTTP/1.1 201 Created\r\nContent-Length: 0\r\n\r\n".to_string();
    let (base_url, handle) =
        serve_once(response).unwrap_or_else(|err| panic!("server failed: {err}"));
    let client = Client::new(ClientConfig {
        base_url,
        token: Some("token".to_string()),
        user_agent: None,
    })
    .unwrap_or_else(|err| panic!("client failed: {err}"));

    client
        .put_repository_bytes("local", "maven", "com/acme/app/1/app-1.jar", "abc".into())
        .await
        .unwrap_or_else(|err| panic!("upload failed: {err}"));

    let request = join_request(handle);
    assert!(request.starts_with("PUT /repositories/local/maven/com/acme/app/1/app-1.jar HTTP/1.1"));
    assert!(request.contains("authorization: Bearer token"));
    assert!(request.ends_with("abc"));
}

#[test]
fn http_error_display_redacts_bearer_tokens_in_body() {
    let err = nr_api::Error::Http(nr_api::HttpError {
        status: reqwest::StatusCode::UNAUTHORIZED,
        body: "bad token Bearer abcdefghijklmnopqrstuvwxyz".to_string(),
    });
    assert!(!err.to_string().contains("abcdefghijklmnopqrstuvwxyz"));
    assert!(err.to_string().contains("Bearer abcd...wxyz"));
}

#[test]
fn dto_deserializes_with_extra_fields() {
    let parsed: nr_api::PackageSearchResult = serde_json::from_value(json!({
        "repository_id": "00000000-0000-0000-0000-000000000001",
        "repository_name": "libs",
        "storage_name": "local",
        "repository_type": "maven",
        "file_name": "app.jar",
        "cache_path": "com/acme/app.jar",
        "size": 10,
        "modified": "2026-04-28T00:00:00+00:00",
        "future_field": "ignored"
    }))
    .unwrap_or_else(|err| panic!("deserialize failed: {err}"));
    assert_eq!(parsed.repository_name, "libs");
}
