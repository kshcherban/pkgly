use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

use clap::Parser;
use pkgly_cli::cli::Cli;
use pkgly_cli::config::{ConfigFile, EnvConfig, ProfileConfig};

struct MockServer {
    base_url: String,
    requests: Arc<Mutex<Vec<String>>>,
    handle: thread::JoinHandle<()>,
}

impl MockServer {
    fn start(responses: Vec<String>) -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let handle = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(_) => return,
                };
                let request = read_request(&mut stream);
                if let Ok(mut guard) = captured.lock() {
                    guard.push(request);
                }
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Ok(Self {
            base_url: format!("http://{addr}"),
            requests,
            handle,
        })
    }

    fn requests(&self) -> Vec<String> {
        match self.requests.lock() {
            Ok(guard) => guard.clone(),
            Err(_) => Vec::new(),
        }
    }

    fn join(self) {
        let _ = self.handle.join();
    }
}

fn read_request(stream: &mut std::net::TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    let mut headers_done = false;
    let mut content_length = 0_usize;
    loop {
        let read = match stream.read(&mut chunk) {
            Ok(value) => value,
            Err(_) => return String::from_utf8_lossy(&buffer).to_string(),
        };
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if !headers_done {
            if let Some(header_end) = find_header_end(&buffer) {
                headers_done = true;
                content_length = parse_content_length(&buffer[..header_end]);
                let body_read = buffer.len().saturating_sub(header_end + 4);
                if body_read >= content_length {
                    break;
                }
            }
        } else if let Some(header_end) = find_header_end(&buffer) {
            let body_read = buffer.len().saturating_sub(header_end + 4);
            if body_read >= content_length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&buffer).to_string()
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(headers: &[u8]) -> usize {
    let text = String::from_utf8_lossy(headers);
    for line in text.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            return value.trim().parse::<usize>().unwrap_or(0);
        }
    }
    0
}

fn json_response(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{body}",
        body.len()
    )
}

fn empty_response(status: &str) -> String {
    format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n")
}

fn config_at(path: &std::path::Path, base_url: &str) {
    let mut config = ConfigFile::default();
    config.active_profile = Some("local".to_string());
    config.profiles.insert(
        "local".to_string(),
        ProfileConfig {
            base_url: Some(base_url.to_string()),
            token: Some("test-token".to_string()),
            default_storage: Some("local".to_string()),
        },
    );
    config
        .save(path)
        .unwrap_or_else(|err| panic!("failed to save config: {err}"));
}

async fn run_cli(args: &[&str]) -> String {
    let cli = Cli::try_parse_from(args).unwrap_or_else(|err| panic!("parse failed: {err}"));
    let mut output = Vec::new();
    pkgly_cli::run(cli, EnvConfig::default(), &mut output)
        .await
        .unwrap_or_else(|err| panic!("run failed: {err}"));
    String::from_utf8(output).unwrap_or_else(|err| panic!("output was not utf8: {err}"))
}

#[tokio::test]
async fn repo_list_command_uses_configured_server_and_token() {
    let server = MockServer::start(vec![json_response("200 OK", "[]")]).unwrap_or_else(|err| {
        panic!("mock server failed: {err}");
    });
    let temp = tempfile::tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
    let config_path = temp.path().join("config.toml");
    config_at(&config_path, &server.base_url);

    let output = run_cli(&[
        "pkglyctl",
        "--config",
        config_path.to_string_lossy().as_ref(),
        "--output",
        "json",
        "repo",
        "list",
    ])
    .await;

    assert_eq!(output, "[]\n");
    let requests = server.requests();
    server.join();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /api/repository/list HTTP/1.1"));
    assert!(requests[0].contains("authorization: Bearer test-token"));
}

#[tokio::test]
async fn auth_login_stores_created_token_without_password() {
    let token_body = "{\"id\":1,\"token\":\"created-token\",\"expires_at\":null}";
    let server = MockServer::start(vec![
        "HTTP/1.1 200 OK\r\nSet-Cookie: session=session123; Path=/\r\nContent-Length: 2\r\nContent-Type: application/json\r\n\r\n{}".to_string(),
        json_response("200 OK", token_body),
    ])
    .unwrap_or_else(|err| panic!("mock server failed: {err}"));
    let temp = tempfile::tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
    let config_path = temp.path().join("config.toml");
    let mut config = ConfigFile::default();
    config.active_profile = Some("local".to_string());
    config.profiles.insert(
        "local".to_string(),
        ProfileConfig {
            base_url: Some(server.base_url.clone()),
            token: None,
            default_storage: None,
        },
    );
    config
        .save(&config_path)
        .unwrap_or_else(|err| panic!("failed to save config: {err}"));

    let output = run_cli(&[
        "pkglyctl",
        "--config",
        config_path.to_string_lossy().as_ref(),
        "auth",
        "login",
        "--username",
        "admin",
        "--password",
        "secret",
    ])
    .await;

    assert_eq!(output, "login complete\n");
    let saved = ConfigFile::load_or_default(&config_path)
        .unwrap_or_else(|err| panic!("failed to reload config: {err}"));
    let profile = saved
        .profiles
        .get("local")
        .unwrap_or_else(|| panic!("local profile missing"));
    assert_eq!(profile.token.as_deref(), Some("created-token"));
    let content = std::fs::read_to_string(&config_path)
        .unwrap_or_else(|err| panic!("failed to read config: {err}"));
    assert!(!content.contains("secret"));
    let requests = server.requests();
    server.join();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("POST /api/user/login HTTP/1.1"));
    assert!(requests[1].starts_with("POST /api/user/token/create HTTP/1.1"));
    assert!(requests[1].contains("cookie: session=session123"));
}

#[tokio::test]
async fn package_download_streams_repository_route_body() {
    let server = MockServer::start(vec![
        "HTTP/1.1 200 OK\r\nContent-Length: 7\r\n\r\npayload".to_string(),
    ])
    .unwrap_or_else(|err| panic!("mock server failed: {err}"));
    let temp = tempfile::tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
    let config_path = temp.path().join("config.toml");
    config_at(&config_path, &server.base_url);

    let output = run_cli(&[
        "pkglyctl",
        "--config",
        config_path.to_string_lossy().as_ref(),
        "package",
        "download",
        "local/libs",
        "a/b.jar",
    ])
    .await;

    assert_eq!(output, "payload");
    let requests = server.requests();
    server.join();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /repositories/local/libs/a/b.jar HTTP/1.1"));
}

#[tokio::test]
async fn repo_delete_requires_yes_and_calls_delete_endpoint() {
    let server = MockServer::start(vec![empty_response("204 No Content")])
        .unwrap_or_else(|err| panic!("mock server failed: {err}"));
    let temp = tempfile::tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
    let config_path = temp.path().join("config.toml");
    config_at(&config_path, &server.base_url);

    let output = run_cli(&[
        "pkglyctl",
        "--config",
        config_path.to_string_lossy().as_ref(),
        "repo",
        "delete",
        "00000000-0000-0000-0000-000000000001",
        "--yes",
    ])
    .await;

    assert_eq!(output, "repository deleted\n");
    let requests = server.requests();
    server.join();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0]
            .starts_with("DELETE /api/repository/00000000-0000-0000-0000-000000000001 HTTP/1.1")
    );
}

#[tokio::test]
async fn maven_upload_puts_file_to_repository_route() {
    let server = MockServer::start(vec![empty_response("201 Created")])
        .unwrap_or_else(|err| panic!("mock server failed: {err}"));
    let temp = tempfile::tempdir().unwrap_or_else(|err| panic!("tempdir failed: {err}"));
    let config_path = temp.path().join("config.toml");
    config_at(&config_path, &server.base_url);
    let artifact = temp.path().join("app.jar");
    std::fs::write(&artifact, "jar-bytes").unwrap_or_else(|err| panic!("write failed: {err}"));

    let output = run_cli(&[
        "pkglyctl",
        "--config",
        config_path.to_string_lossy().as_ref(),
        "package",
        "upload",
        "maven",
        "local/libs",
        "com/acme/app/1/app-1.jar",
        artifact.to_string_lossy().as_ref(),
    ])
    .await;

    assert_eq!(output, "upload complete\n");
    let requests = server.requests();
    server.join();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].starts_with("PUT /repositories/local/libs/com/acme/app/1/app-1.jar HTTP/1.1")
    );
    assert!(requests[0].ends_with("jar-bytes"));
}
