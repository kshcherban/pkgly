// ABOUTME: Owns an isolated MinIO server for storage deletion failure tests.
// ABOUTME: Controls repository deletion through real policies and removes the server on drop.
use std::{
    process::{Command, Stdio},
    time::Duration,
};

use serde_json::{Value, json};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};
use uuid::Uuid;

pub(super) struct TestMinio<'docker> {
    container: Container<'docker, GenericImage>,
}

impl<'docker> TestMinio<'docker> {
    pub(super) async fn start(docker: &'docker Cli) -> Self {
        let image = GenericImage::new("minio/minio", "RELEASE.2025-03-12T18-04-18Z")
            .with_env_var("MINIO_ROOT_USER", "minioadmin")
            .with_env_var("MINIO_ROOT_PASSWORD", "minioadmin")
            .with_exposed_port(9000);
        let fixture = Self {
            container: docker.run((image, vec!["server".into(), "/data".into()])),
        };
        fixture.wait_until_ready().await;
        fixture.configure();
        fixture
    }

    fn configure(&self) {
        self.mc(
            &[
                "alias",
                "set",
                "fixture",
                "http://127.0.0.1:9000",
                "minioadmin",
                "minioadmin",
            ],
            None,
        );
        self.mc(&["mb", "fixture/packages"], None);
        self.mc(
            &[
                "admin",
                "user",
                "add",
                "fixture",
                "cleanup-user",
                "cleanup-password",
            ],
            None,
        );
        self.mc(
            &[
                "admin",
                "policy",
                "attach",
                "fixture",
                "readwrite",
                "--user",
                "cleanup-user",
            ],
            None,
        );
    }

    async fn wait_until_ready(&self) {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(1))
            .build()
            .expect("health client");
        let url = format!("{}/minio/health/live", self.endpoint());
        for _ in 0..60 {
            if let Ok(response) = client.get(&url).send().await {
                if response.status().is_success() {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("MinIO did not become ready");
    }

    pub(super) fn id(&self) -> &str {
        self.container.id()
    }

    fn endpoint(&self) -> String {
        format!(
            "http://127.0.0.1:{}",
            self.container.get_host_port_ipv4(9000)
        )
    }

    pub(super) fn config(&self) -> Value {
        json!({
            "type": "S3",
            "settings": {
                "bucket_name": "packages",
                "region": "us-east-1",
                "endpoint": self.endpoint(),
                "credentials": { "access_key": "cleanup-user", "secret_key": "cleanup-password" },
                "path_style": true,
                "cache": { "enabled": false }
            }
        })
    }

    pub(super) fn deny_deletion(&self, repository: Uuid) {
        let policy = json!({
            "Version": "2012-10-17",
            "Statement": [{
                "Effect": "Deny",
                "Action": ["s3:DeleteObject"],
                "Resource": [format!("arn:aws:s3:::packages/{repository}/*")]
            }]
        });
        self.mc(
            &[
                "admin",
                "policy",
                "create",
                "fixture",
                "deny-deletion",
                "/dev/stdin",
            ],
            Some(&policy),
        );
        self.mc(
            &[
                "admin",
                "policy",
                "attach",
                "fixture",
                "deny-deletion",
                "--user",
                "cleanup-user",
            ],
            None,
        );
    }

    pub(super) fn allow_deletion(&self) {
        self.mc(
            &[
                "admin",
                "policy",
                "detach",
                "fixture",
                "deny-deletion",
                "--user",
                "cleanup-user",
            ],
            None,
        );
    }

    pub(super) fn objects(&self) -> Vec<String> {
        self.mc(&["ls", "--recursive", "fixture/packages"], None)
            .lines()
            .map(|line| {
                let object: Value = serde_json::from_str(line).expect("object entry");
                object["key"].as_str().expect("object key").to_string()
            })
            .collect()
    }

    fn mc(&self, args: &[&str], input: Option<&Value>) -> String {
        let mut child = Command::new("docker")
            .args(["exec", "-i", self.id(), "mc", "--json"])
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start MinIO command");
        let mut stdin = child.stdin.take().expect("MinIO command stdin");
        if let Some(input) = input {
            serde_json::to_writer(&mut stdin, input).expect("write MinIO policy");
        }
        drop(stdin);
        let output = child.wait_with_output().expect("wait for MinIO command");
        assert!(
            output.status.success(),
            "MinIO command failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("MinIO command output")
    }
}
