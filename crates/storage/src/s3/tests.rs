#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::{
    AdaptiveBufferConfig, BodyRetrievalStrategy, CustomRegion, DEFAULT_MAX_BUFFERED_OBJECT_BYTES,
    S3CacheConfig, S3Config, S3Credentials, S3DiskCache, S3StorageRegion,
};
use bytes::Bytes;
use tempfile::tempdir;
use tokio::{
    fs,
    time::{Duration, sleep},
};

#[test]
fn static_credentials_detected() {
    let creds = S3Credentials::new_access_key("AKIA", "secret");
    let static_keys = creds.static_keys();
    assert!(static_keys.is_some());
    let keys = static_keys.unwrap();
    assert_eq!(keys.access_key, "AKIA");
    assert_eq!(keys.secret_key, "secret");
    assert!(keys.session_token.is_none());
}

#[test]
fn missing_keys_use_default_chain() {
    let creds = S3Credentials::default();
    assert!(creds.static_keys().is_none());
}

#[test]
fn role_detection_prefers_non_empty_strings() {
    let creds = S3Credentials {
        role_arn: Some("arn:aws:iam::123:role/demo".into()),
        role_session_name: Some("pkgly".into()),
        ..Default::default()
    };
    let role = creds.role_to_assume().expect("role should be detected");
    assert_eq!(role.role_arn, "arn:aws:iam::123:role/demo");
    assert_eq!(role.session_name.as_deref(), Some("pkgly"));

    let empty_role = S3Credentials {
        role_arn: Some("   ".into()),
        ..Default::default()
    };
    assert!(empty_role.role_to_assume().is_none());
}

#[test]
fn custom_region_returns_endpoint_and_name() {
    let config = S3Config {
        bucket_name: "pkgly".into(),
        region: Some(S3StorageRegion::UsEast1),
        custom_region: Some(CustomRegion {
            custom_region: Some("minio".into()),
            endpoint: "https://minio.local".parse().unwrap(),
        }),
        credentials: S3Credentials::default(),
        path_style: true,
        cache: S3CacheConfig::default(),
        adaptive_buffer: AdaptiveBufferConfig::default(),
    };

    let resolved = config
        .resolved_region()
        .expect("custom region should resolve");
    assert_eq!(resolved.as_ref(), "minio");
    assert!(config.custom_endpoint().is_some());
}

#[test]
fn body_strategy_caches_small_objects() {
    let limit = DEFAULT_MAX_BUFFERED_OBJECT_BYTES;
    let result = BodyRetrievalStrategy::from_content_length(Some(limit - 1), true, limit);
    assert_eq!(result, BodyRetrievalStrategy::BufferAndCache);
}

#[test]
fn body_strategy_streams_large_objects() {
    let limit = DEFAULT_MAX_BUFFERED_OBJECT_BYTES;
    let result = BodyRetrievalStrategy::from_content_length(Some(limit + 1), true, limit);
    assert_eq!(result, BodyRetrievalStrategy::StreamWithoutCache);
}

#[test]
fn body_strategy_streams_when_cache_disabled() {
    let result = BodyRetrievalStrategy::from_content_length(Some(1), false, 1);
    assert_eq!(result, BodyRetrievalStrategy::StreamWithoutCache);
}

#[test]
fn body_strategy_streams_when_size_unknown() {
    let limit = DEFAULT_MAX_BUFFERED_OBJECT_BYTES;
    let result = BodyRetrievalStrategy::from_content_length(None, true, limit);
    assert_eq!(result, BodyRetrievalStrategy::StreamWithoutCache);
}

fn cache_config_with_dir(dir: &std::path::Path) -> S3CacheConfig {
    S3CacheConfig {
        enabled: true,
        path: Some(dir.to_path_buf()),
        max_bytes: 8,
        max_entries: 4,
    }
}

#[tokio::test]
async fn disk_cache_retries_failed_deletions_on_next_put() {
    let temp_dir = tempdir().expect("tempdir");
    let cache = S3DiskCache::new(&cache_config_with_dir(temp_dir.path()), "test-cache")
        .await
        .expect("cache");

    cache
        .put("first", Bytes::from_static(b"abcdefgh"), None)
        .await
        .expect("initial write");

    let relative = S3DiskCache::hashed_filename("first");
    let disk_path = cache.dir.join(&relative);
    fs::remove_file(&disk_path)
        .await
        .expect("remove original file");
    fs::create_dir_all(&disk_path)
        .await
        .expect("replace file with dir");

    cache
        .put("second", Bytes::from_static(b"ijklmnop"), None)
        .await
        .expect("evict first entry");

    let metadata = fs::metadata(&disk_path).await.expect("metadata");
    assert!(metadata.is_dir(), "corrupted entry stays on disk");

    fs::remove_dir_all(&disk_path)
        .await
        .expect("cleanup dir before retry");
    fs::File::create(&disk_path)
        .await
        .expect("recreate file so deletion can succeed");

    sleep(Duration::from_millis(150)).await;

    cache
        .put("third", Bytes::from_static(b"qrstuvwx"), None)
        .await
        .expect("trigger retry");

    let exists = fs::try_exists(&disk_path).await.expect("exists check");
    assert!(!exists, "failed deletions get retried before new puts");
}

#[test]
fn adaptive_buffer_respects_pressure_threshold() {
    let config = AdaptiveBufferConfig {
        min_buffer_bytes: 1024 * 1024,
        max_buffer_bytes: 16 * 1024 * 1024,
        memory_pressure_threshold: 0.5,
    };

    assert_eq!(config.limit_for_pressure(0.0), 16 * 1024 * 1024);
    let mid = config.limit_for_pressure(0.25);
    assert!(mid < 16 * 1024 * 1024 && mid > 1024 * 1024);
    assert_eq!(config.limit_for_pressure(0.5), 1024 * 1024);
    assert_eq!(config.limit_for_pressure(0.9), 1024 * 1024);
}

// ---------------------------------------------------------------------------
// delete_repository integration tests against a lightweight mock S3 endpoint.
// ---------------------------------------------------------------------------
use super::{S3Storage, S3StorageInner};
use crate::{Storage, StorageConfigInner};
use aws_config::BehaviorVersion;
use aws_credential_types::{Credentials as AwsCredentials, provider::SharedCredentialsProvider};
use aws_sdk_s3::config::Builder as S3ConfigBuilder;
use aws_types::region::Region;
use bytes::Buf;
use http_body_util::{BodyExt, Full};
use hyper::{
    Request, Response, StatusCode, body::Incoming, server::conn::http1, service::service_fn,
};
use hyper_util::rt::TokioIo;
use parking_lot::Mutex;
use std::{collections::VecDeque, convert::Infallible, net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use uuid::Uuid;

type RecordedBody = bytes::Bytes;
type Responder =
    Box<dyn FnMut(Request<RecordedBody>) -> Response<Full<bytes::Bytes>> + Send + 'static>;

#[derive(Clone, Debug)]
struct RecordedRequest {
    method: hyper::Method,
    uri: hyper::Uri,
    body: RecordedBody,
}

struct MockS3Server {
    address: SocketAddr,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    shutdown: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}

impl MockS3Server {
    async fn start(responders: Vec<Responder>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind mock s3");
        let address = listener.local_addr().expect("address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let responder_queue = Arc::new(Mutex::new(VecDeque::from(responders)));

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let requests_clone = Arc::clone(&requests);
        let responders_clone = Arc::clone(&responder_queue);

        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accept = listener.accept() => {
                        let (stream, _) = match accept {
                            Ok(ok) => ok,
                            Err(err) => {
                                eprintln!("mock s3 accept error: {err}");
                                continue;
                            }
                        };
                        let requests = Arc::clone(&requests_clone);
                        let responders = Arc::clone(&responders_clone);
                        tokio::spawn(async move {
                            let service = service_fn(move |req: Request<Incoming>| {
                                handle_request(req, Arc::clone(&requests), Arc::clone(&responders))
                            });
                            if let Err(err) = http1::Builder::new()
                                .serve_connection(TokioIo::new(stream), service)
                                .await
                            {
                                eprintln!("mock s3 connection error: {err}");
                            }
                        });
                    }
                }
            }
        });

        Self {
            address,
            requests,
            shutdown: shutdown_tx,
            task,
        }
    }

    fn endpoint(&self) -> String {
        format!("http://{}", self.address)
    }

    fn take_requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().clone()
    }

    async fn shutdown(self) {
        let _ = self.shutdown.send(());
        let _ = self.task.await;
    }
}

async fn handle_request(
    mut req: Request<Incoming>,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    responders: Arc<Mutex<VecDeque<Responder>>>,
) -> Result<Response<Full<bytes::Bytes>>, Infallible> {
    let body_bytes = req.body_mut().collect().await.unwrap().to_bytes();
    let (parts, _) = req.into_parts();
    let req_with_body = Request::from_parts(parts, body_bytes.clone());

    requests.lock().push(RecordedRequest {
        method: req_with_body.method().clone(),
        uri: req_with_body.uri().clone(),
        body: body_bytes.clone(),
    });

    let mut queue = responders.lock();
    let resp = if let Some(responder) = queue.front_mut() {
        responder(req_with_body)
    } else {
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Full::new(bytes::Bytes::from_static(
                b"no responder for request",
            )))
            .unwrap()
    };
    // pop only after a successful response to preserve strict ordering on errors
    queue.pop_front();
    Ok(resp)
}

fn build_s3_storage(endpoint: &str, bucket: &str) -> S3Storage {
    let mut storage_config = StorageConfigInner::test_config();
    storage_config.storage_type = "s3".into();

    let client_config = S3ConfigBuilder::new()
        .region(Region::new("us-east-1"))
        .behavior_version(BehaviorVersion::latest())
        .force_path_style(true)
        .endpoint_url(endpoint)
        .credentials_provider(SharedCredentialsProvider::new(AwsCredentials::new(
            "AKIA", "SECRET", None, None, "mock",
        )))
        .build();

    let client = aws_sdk_s3::Client::from_conf(client_config);
    let config = S3Config {
        bucket_name: bucket.into(),
        region: Some(S3StorageRegion::UsEast1),
        custom_region: Some(CustomRegion {
            custom_region: Some("us-east-1".into()),
            endpoint: endpoint.parse().unwrap(),
        }),
        credentials: S3Credentials::new_access_key("AKIA", "SECRET"),
        path_style: true,
        cache: S3CacheConfig::default(),
        adaptive_buffer: AdaptiveBufferConfig::default(),
    };

    let inner = S3StorageInner {
        config,
        storage_config,
        client,
        cache: None,
    };

    S3Storage::from(inner)
}

fn list_response_body(prefix: &str, keys: &[&str], truncated: bool, token: Option<&str>) -> String {
    let contents = keys
        .iter()
        .map(|k| format!("<Contents><Key>{}</Key></Contents>", k))
        .collect::<Vec<_>>()
        .join("");
    let token_xml = token
        .map(|t| format!("<NextContinuationToken>{t}</NextContinuationToken>"))
        .unwrap_or_default();
    format!(
        r#"<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
    <Name>mock-bucket</Name>
    <Prefix>{prefix}</Prefix>
    <KeyCount>{}</KeyCount>
    <IsTruncated>{}</IsTruncated>
    {contents}
    {token_xml}
</ListBucketResult>"#,
        keys.len(),
        if truncated { "true" } else { "false" }
    )
}

fn delete_ok_response() -> Response<Full<bytes::Bytes>> {
    Response::builder()
        .status(StatusCode::OK)
        .body(Full::new(bytes::Bytes::from_static(
            br#"<DeleteResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/"></DeleteResult>"#,
        )))
        .unwrap()
}

fn respond_list(body: String) -> Responder {
    Box::new(move |_| {
        Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(bytes::Bytes::from(body.clone())))
            .unwrap()
    })
}

fn respond_delete(assert_keys: Vec<String>) -> Responder {
    Box::new(move |req| {
        let body = req.into_body();
        let body_text = std::str::from_utf8(body.chunk()).expect("utf8 delete body");
        for key in &assert_keys {
            assert!(
                body_text.contains(key),
                "delete payload should contain key {key}, payload was {body_text}"
            );
        }
        delete_ok_response()
    })
}

fn parse_deleted_keys(body: &RecordedBody) -> Vec<String> {
    let text = String::from_utf8_lossy(body);
    text.split("<Key>")
        .skip(1)
        .filter_map(|part| part.split("</Key>").next())
        .map(|s| s.to_string())
        .collect()
}

#[tokio::test]
async fn delete_repository_removes_all_files() {
    let repository = Uuid::new_v4();
    let key_one = format!("{repository}/packages/a.bin");
    let key_two = format!("{repository}/packages/nested/b.bin");

    let server = MockS3Server::start(vec![
        respond_list(list_response_body(
            &format!("{repository}/"),
            &[&key_one, &key_two],
            false,
            None,
        )),
        respond_delete(vec![key_one.clone(), key_two.clone()]),
    ])
    .await;

    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");
    storage
        .delete_repository(repository)
        .await
        .expect("delete_repository should succeed");

    let requests = server.take_requests();
    assert_eq!(requests.len(), 2, "one list and one delete call expected");
    let delete_keys = parse_deleted_keys(&requests[1].body);
    assert_eq!(delete_keys.len(), 2);
    assert!(delete_keys.contains(&key_one));
    assert!(delete_keys.contains(&key_two));

    server.shutdown().await;
}

#[tokio::test]
async fn delete_repository_handles_pagination() {
    let repository = Uuid::new_v4();
    let first_page_keys = vec![
        format!("{repository}/page1/one"),
        format!("{repository}/page1/two"),
    ];
    let second_page_keys = vec![format!("{repository}/page2/three")];

    let server = MockS3Server::start(vec![
        respond_list(list_response_body(
            &format!("{repository}/"),
            &first_page_keys
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
            true,
            Some("token-1"),
        )),
        respond_delete(first_page_keys.clone()),
        respond_list(list_response_body(
            &format!("{repository}/"),
            &second_page_keys
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
            false,
            None,
        )),
        respond_delete(second_page_keys.clone()),
    ])
    .await;

    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");
    storage
        .delete_repository(repository)
        .await
        .expect("delete_repository should succeed with pagination");

    let requests = server.take_requests();
    assert_eq!(
        requests.len(),
        4,
        "list/delete/list/delete sequence expected"
    );
    assert!(
        requests[2]
            .uri
            .query()
            .unwrap_or_default()
            .contains("continuation-token=token-1"),
        "second list should carry continuation token"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn delete_repository_is_idempotent_on_empty_prefix() {
    let repository = Uuid::new_v4();
    let server = MockS3Server::start(vec![respond_list(list_response_body(
        &format!("{repository}/"),
        &[],
        false,
        None,
    ))])
    .await;

    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");
    storage
        .delete_repository(repository)
        .await
        .expect("empty prefixes should be handled gracefully");

    let requests = server.take_requests();
    assert_eq!(requests.len(), 1, "only a list request is expected");

    server.shutdown().await;
}

#[tokio::test]
async fn delete_repository_preserves_other_repositories() {
    let repository = Uuid::new_v4();
    let other_repo = Uuid::new_v4();

    let keys = [
        format!("{repository}/packages/a.bin"),
        format!("{other_repo}/packages/should-not-delete"),
    ];

    let server = MockS3Server::start(vec![
        respond_list(list_response_body(
            &format!("{repository}/"),
            &keys.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            false,
            None,
        )),
        respond_delete(vec![format!("{repository}/packages/a.bin")]),
    ])
    .await;

    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");
    storage
        .delete_repository(repository)
        .await
        .expect("delete_repository should ignore other repo keys");

    let requests = server.take_requests();
    assert_eq!(requests.len(), 2);
    let deleted = parse_deleted_keys(&requests[1].body);
    assert_eq!(deleted, vec![format!("{repository}/packages/a.bin")]);
    assert!(
        !deleted
            .iter()
            .any(|key| key.contains(&other_repo.to_string())),
        "keys from other repositories must not be deleted"
    );

    server.shutdown().await;
}
