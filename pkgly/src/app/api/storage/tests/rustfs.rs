// ABOUTME: Owns an isolated RustFS server for storage deletion failure tests.
// ABOUTME: Controls repository deletion through real bucket policies and removes the server on drop.
use std::time::Duration;

use aws_sdk_s3::config::{Credentials, Region};
use serde_json::{Value, json};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};
use uuid::Uuid;

const ACCESS_KEY: &str = "rustfsadmin";
const SECRET_KEY: &str = "rustfsadmin";
const BUCKET: &str = "packages";
const S3_PORT: u16 = 9000;

pub(super) struct TestRustfs<'docker> {
    container: Container<'docker, GenericImage>,
}

impl<'docker> TestRustfs<'docker> {
    pub(super) async fn start(docker: &'docker Cli) -> Self {
        let image = GenericImage::new("rustfs/rustfs", "1.0.0")
            .with_env_var("RUSTFS_ACCESS_KEY", ACCESS_KEY)
            .with_env_var("RUSTFS_SECRET_KEY", SECRET_KEY)
            .with_env_var("RUSTFS_ADDRESS", format!("0.0.0.0:{S3_PORT}"))
            .with_env_var("RUSTFS_CONSOLE_ENABLE", "false")
            .with_env_var("RUSTFS_VOLUMES", "/data")
            .with_exposed_port(S3_PORT);
        let fixture = Self {
            container: docker.run((image, vec!["rustfs".into()])),
        };
        fixture.wait_until_ready().await;
        fixture.configure().await;
        fixture
    }

    async fn configure(&self) {
        let client = self.client();
        for attempt in 0..60 {
            match client.create_bucket().bucket(BUCKET).send().await {
                Ok(_) => return,
                Err(error) => {
                    let already_exists = error.as_service_error().is_some_and(|service| {
                        service.is_bucket_already_owned_by_you()
                            || service.is_bucket_already_exists()
                    });
                    if already_exists {
                        return;
                    }
                    if attempt == 59 {
                        panic!("failed to create RustFS bucket: {error}");
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }

    async fn wait_until_ready(&self) {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(1))
            .build()
            .expect("health client");
        let url = format!("{}/health", self.endpoint());
        for _ in 0..600 {
            if let Ok(response) = client.get(&url).send().await {
                if response.status().is_success() {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("RustFS did not become ready");
    }

    pub(super) fn id(&self) -> &str {
        self.container.id()
    }

    fn endpoint(&self) -> String {
        format!(
            "http://127.0.0.1:{}",
            self.container.get_host_port_ipv4(S3_PORT)
        )
    }

    fn client(&self) -> aws_sdk_s3::Client {
        let config = aws_sdk_s3::Config::builder()
            .credentials_provider(Credentials::new(
                ACCESS_KEY,
                SECRET_KEY,
                None,
                None,
                "rustfs-test",
            ))
            .region(Region::new("us-east-1"))
            .endpoint_url(self.endpoint())
            .force_path_style(true)
            .behavior_version_latest()
            .build();
        aws_sdk_s3::Client::from_conf(config)
    }

    pub(super) fn config(&self) -> Value {
        json!({
            "type": "S3",
            "settings": {
                "bucket_name": BUCKET,
                "region": "us-east-1",
                "endpoint": self.endpoint(),
                "credentials": { "access_key": ACCESS_KEY, "secret_key": SECRET_KEY },
                "path_style": true,
                "cache": { "enabled": false }
            }
        })
    }

    pub(super) async fn deny_deletion(&self, repository: Uuid) {
        let policy = json!({
            "Version": "2012-10-17",
            "Statement": [{
                "Sid": "DenyRepositoryDeletion",
                "Effect": "Deny",
                "Principal": { "AWS": "*" },
                "Action": ["s3:DeleteObject"],
                "Resource": [format!("arn:aws:s3:::{BUCKET}/{repository}/*")]
            }]
        });
        self.client()
            .put_bucket_policy()
            .bucket(BUCKET)
            .policy(policy.to_string())
            .send()
            .await
            .expect("apply RustFS bucket policy");
    }

    pub(super) async fn allow_deletion(&self) {
        self.client()
            .delete_bucket_policy()
            .bucket(BUCKET)
            .send()
            .await
            .expect("remove RustFS bucket policy");
    }

    pub(super) async fn objects(&self) -> Vec<String> {
        let client = self.client();
        let mut keys = Vec::new();
        let mut continuation: Option<String> = None;
        loop {
            let mut request = client.list_objects_v2().bucket(BUCKET);
            if let Some(token) = continuation.as_deref() {
                request = request.continuation_token(token);
            }
            let response = request.send().await.expect("list RustFS objects");
            for object in response.contents() {
                if let Some(key) = object.key() {
                    keys.push(key.to_string());
                }
            }
            if response.is_truncated() == Some(true) {
                continuation = response.next_continuation_token().map(str::to_owned);
            } else {
                break;
            }
        }
        keys.sort();
        keys
    }
}
