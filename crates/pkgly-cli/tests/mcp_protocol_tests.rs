// ABOUTME: Exercises the pkglyctl MCP protocol through its real stdio subprocess.
// ABOUTME: Verifies discovery, local actions, validation, secret masking, and shutdown.
#![allow(clippy::expect_used)]
use std::{path::Path, process::Stdio, time::Duration};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    process::{Child, ChildStdin, ChildStdout, Command},
    time::timeout,
};

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    id: u64,
}

impl Client {
    async fn start(config: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_pkglyctl"))
            .args([
                "--config",
                config.to_str().expect("config path"),
                "mcp",
                "serve",
            ])
            .env_remove("PKGLY_URL")
            .env_remove("PKGLY_TOKEN")
            .env_remove("PKGLY_PROFILE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn MCP server");
        let mut client = Self {
            stdin: child.stdin.take().expect("stdin"),
            stdout: BufReader::new(child.stdout.take().expect("stdout")).lines(),
            child,
            id: 0,
        };
        let response = client
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2025-11-25", "capabilities": {},
                    "clientInfo": {"name": "pkgly-protocol-test", "version": "1"},
                }),
            )
            .await;
        assert!(
            response["result"]["capabilities"]["tools"].is_object(),
            "{response}"
        );
        client
            .send(json!({"jsonrpc":"2.0", "method":"notifications/initialized"}))
            .await;
        client
    }

    async fn send(&mut self, message: Value) {
        let mut bytes = serde_json::to_vec(&message).expect("encode request");
        bytes.push(b'\n');
        self.stdin.write_all(&bytes).await.expect("write request");
        self.stdin.flush().await.expect("flush request");
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        self.send(json!({"jsonrpc":"2.0", "id":self.id, "method":method, "params":params}))
            .await;
        loop {
            let line = timeout(Duration::from_secs(15), self.stdout.next_line())
                .await
                .expect("MCP response timed out")
                .expect("read response")
                .expect("MCP stdout closed");
            let response: Value =
                serde_json::from_str(&line).expect("stdout must contain only JSON-RPC");
            assert_eq!(
                response["jsonrpc"], "2.0",
                "stdout must contain only JSON-RPC"
            );
            if response["id"] == self.id {
                return response;
            }
        }
    }

    async fn tool(&mut self, name: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({"name":name, "arguments":arguments}))
            .await
    }
}

#[tokio::test]
async fn discovery_and_native_commands_work_without_server_credentials() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config = dir.path().join("config.toml");
    std::fs::write(
        &config,
        "active_profile = 'local'\n[profiles.local]\nbase_url = 'https://pkgly.example'\n",
    )
    .expect("write config");
    let mut client = Client::start(&config).await;
    let listing = client.request("tools/list", json!({})).await;
    let tools = listing["result"]["tools"].as_array().expect("tools");
    assert_eq!(tools.len(), 38);
    let search = tools
        .iter()
        .find(|tool| tool["name"] == "package_search")
        .expect("search tool");
    assert_eq!(
        search["inputSchema"]["properties"]["query"]["type"],
        "string"
    );
    assert_eq!(search["annotations"]["readOnlyHint"], true);
    let download = tools
        .iter()
        .find(|tool| tool["name"] == "package_download")
        .expect("download tool");
    assert!(
        download["inputSchema"]["required"]
            .as_array()
            .expect("required fields")
            .contains(&json!("output_file"))
    );
    let response = client
        .tool("native_npm", json!({"repository":"local/npm"}))
        .await;
    assert_ne!(response["result"]["isError"], true, "{response}");
    assert!(
        response["result"]["structuredContent"]["message"]
            .as_str()
            .expect("command")
            .contains("https://pkgly.example/repositories/local/npm/")
    );
}

#[tokio::test]
async fn profile_changes_are_reloaded_and_credentials_are_masked() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config = dir.path().join("config.toml");
    let mut client = Client::start(&config).await;
    let secret = "mcp-test-credential-must-never-appear";
    let response = client
        .tool("auth_set_token", json!({"token":secret, "profile":"local"}))
        .await;
    assert_ne!(response["result"]["isError"], true, "{response}");
    let response = client.tool("profile_show", json!({})).await;
    assert_eq!(response["result"]["structuredContent"]["name"], "local");
    assert!(!response.to_string().contains(secret));
    let response = client.tool("auth_logout", json!({})).await;
    assert_ne!(response["result"]["isError"], true, "{response}");
    let response = client.tool("profile_show", json!({})).await;
    assert!(response["result"]["structuredContent"]["token"].is_null());
    let response = client
        .tool("profile_remove", json!({"profile":"local"}))
        .await;
    assert_ne!(response["result"]["isError"], true, "{response}");
    let response = client.tool("profile_list", json!({})).await;
    assert_eq!(response["result"]["structuredContent"]["items"], json!([]));
}

#[tokio::test]
async fn validation_rejects_missing_inputs_and_unconfirmed_deletion() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config = dir.path().join("config.toml");
    std::fs::write(
        &config,
        "active_profile = 'local'\n[profiles.local]\nbase_url = 'http://127.0.0.1:1'\n",
    )
    .expect("write config");
    let mut client = Client::start(&config).await;
    for (name, arguments) in [
        (
            "package_download",
            json!({"repository":"local/maven", "path":"file.jar"}),
        ),
        ("auth_login", json!({"username":"admin"})),
        ("does_not_exist", json!({})),
    ] {
        let response = client.tool(name, arguments).await;
        assert!(
            response["error"].is_object() || response["result"]["isError"] == true,
            "{response}"
        );
    }
    let response = client
        .tool("repo_delete", json!({"repository":"local/maven"}))
        .await;
    assert_eq!(response["result"]["isError"], true, "{response}");
    assert!(
        response.to_string().contains("requires --yes"),
        "{response}"
    );
}

#[tokio::test]
async fn configuration_errors_do_not_echo_stored_credentials() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config = dir.path().join("config.toml");
    let secret = "profile-credential-must-not-be-echoed";
    std::fs::write(&config, format!("[profiles.local]\ntoken = {secret}\n"))
        .expect("write invalid config");
    let mut client = Client::start(&config).await;
    let response = client.tool("profile_list", json!({})).await;
    assert_eq!(response["result"]["isError"], true, "{response}");
    assert_eq!(
        response["result"]["structuredContent"]["message"],
        "invalid CLI configuration (TOML)"
    );
    assert!(!response.to_string().contains(secret));
}

#[tokio::test]
async fn server_exits_cleanly_when_client_closes_stdin() {
    let dir = tempfile::tempdir().expect("temp dir");
    let client = Client::start(&dir.path().join("config.toml")).await;
    let Client {
        mut child, stdin, ..
    } = client;
    drop(stdin);
    let status = timeout(Duration::from_secs(10), child.wait())
        .await
        .expect("shutdown timeout")
        .expect("wait");
    assert!(status.success(), "{status}");
}
