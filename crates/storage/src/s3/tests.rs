// ABOUTME: Exercises S3 configuration, caching, and storage behavior.
// ABOUTME: Provides a deterministic HTTP S3 test service for protocol-level tests.
#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::{
    AdaptiveBufferConfig, BodyRetrievalStrategy, CustomRegion, DEFAULT_MAX_BUFFERED_OBJECT_BYTES,
    MemorySnapshot, MemorySnapshotCache, S3CacheConfig, S3Config, S3Credentials, S3DiskCache,
    default_cache_dir, resolve_cache_dir,
};
use aws_smithy_runtime_api::{
    client::{
        orchestrator::HttpResponse as SmithyHttpResponse, result::SdkError as SmithySdkError,
    },
    http::StatusCode as SmithyStatusCode,
};
use aws_smithy_types::{
    body::SdkBody,
    error::{ErrorMetadata, metadata::ProvideErrorMetadata},
};
use bytes::Bytes;
use chrono::{FixedOffset, TimeZone};
use tempfile::tempdir;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
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
        region: Some("us-east-1".into()),
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
fn raw_region_values_are_passed_through() {
    let config = S3Config {
        bucket_name: "pkgly".into(),
        region: Some("eu-central-99".into()),
        custom_region: None,
        credentials: S3Credentials::default(),
        path_style: true,
        cache: S3CacheConfig::default(),
        adaptive_buffer: AdaptiveBufferConfig::default(),
    };

    assert_eq!(
        config.resolved_region().expect("region").as_ref(),
        "eu-central-99"
    );
}

#[test]
fn copy_source_encodes_bucket_and_key() {
    assert_eq!(
        super::copy_source("bucket", "folder/file name+one"),
        "/bucket/folder%2Ffile%20name%2Bone"
    );
}

#[test]
fn sdk_error_messages_keep_actionable_kinds() {
    assert_eq!(
        super::S3StorageError::aws_message("PreconditionFailed: status code: 412").kind(),
        Some(super::S3ErrorKind::Conflict)
    );
    assert_eq!(
        super::S3StorageError::aws_message("AccessDenied: status code: 403").kind(),
        Some(super::S3ErrorKind::AccessDenied)
    );
    assert_eq!(
        super::S3StorageError::aws_message("NoSuchBucket").kind(),
        Some(super::S3ErrorKind::NotFound)
    );
    assert_eq!(
        super::S3StorageError::aws_message("HTTP status 404").kind(),
        Some(super::S3ErrorKind::NotFound)
    );
    assert!(super::S3StorageError::aws_message("SlowDown: status code: 429").is_retryable());
    assert!(super::S3StorageError::aws_message("status code: 503").is_retryable());
    assert!(super::S3StorageError::aws_message("dispatch failure").is_retryable());
    assert!(super::S3StorageError::aws_message("operation deadline exceeded").is_retryable());
}

#[derive(Debug)]
struct MetadataOnlyError(ErrorMetadata);

impl std::fmt::Display for MetadataOnlyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("opaque service failure")
    }
}

impl std::error::Error for MetadataOnlyError {}

impl ProvideErrorMetadata for MetadataOnlyError {
    fn meta(&self) -> &ErrorMetadata {
        &self.0
    }
}

fn smithy_response(status: u16) -> SmithyHttpResponse {
    SmithyHttpResponse::new(
        SmithyStatusCode::try_from(status).expect("valid status"),
        SdkBody::empty(),
    )
}

#[test]
fn sdk_error_classification_uses_smithy_metadata_and_status() {
    let denied = SmithySdkError::service_error(
        MetadataOnlyError(ErrorMetadata::builder().code("AccessDenied").build()),
        smithy_response(500),
    );
    assert_eq!(
        super::S3StorageError::from_sdk_error(denied).kind(),
        Some(super::S3ErrorKind::AccessDenied)
    );

    let conflict = SmithySdkError::service_error(
        MetadataOnlyError(ErrorMetadata::builder().code("OpaqueCode").build()),
        smithy_response(412),
    );
    assert_eq!(
        super::S3StorageError::from_sdk_error(conflict).kind(),
        Some(super::S3ErrorKind::Conflict)
    );
}

#[test]
fn timeout_profiles_define_attempt_and_operation_deadlines() {
    let control = super::control_timeout_config();
    assert_eq!(
        control.operation_attempt_timeout(),
        Some(Duration::from_secs(30))
    );
    assert_eq!(control.operation_timeout(), Some(Duration::from_secs(90)));

    let copy = super::copy_timeout_config();
    assert_eq!(
        copy.operation_attempt_timeout(),
        Some(Duration::from_secs(5 * 60))
    );
    assert_eq!(copy.operation_timeout(), Some(Duration::from_secs(20 * 60)));

    let streaming = super::streaming_timeout_config();
    assert_eq!(streaming.operation_attempt_timeout(), None);
    assert_eq!(streaming.operation_timeout(), None);
    assert_eq!(streaming.connect_timeout(), Some(Duration::from_secs(5)));
    assert_eq!(streaming.read_timeout(), Some(Duration::from_secs(30)));
}

#[test]
fn legacy_region_enum_values_deserialize_to_raw_ids() {
    let config: S3Config = serde_json::from_value(serde_json::json!({
        "bucket_name": "pkgly",
        "region": "UsEast1",
        "credentials": {},
    }))
    .expect("legacy region should remain readable");

    assert_eq!(config.region.as_deref(), Some("us-east-1"));
}

#[test]
fn every_legacy_region_token_is_migrated_by_deserializer() {
    let mappings = [
        ("UsEast1", "us-east-1"),
        ("UsEast2", "us-east-2"),
        ("UsWest1", "us-west-1"),
        ("UsWest2", "us-west-2"),
        ("CaCentral1", "ca-central-1"),
        ("AfSouth1", "af-south-1"),
        ("ApEast1", "ap-east-1"),
        ("ApSouth1", "ap-south-1"),
        ("ApNortheast1", "ap-northeast-1"),
        ("ApNortheast2", "ap-northeast-2"),
        ("ApNortheast3", "ap-northeast-3"),
        ("ApSoutheast1", "ap-southeast-1"),
        ("ApSoutheast2", "ap-southeast-2"),
        ("CnNorth1", "cn-north-1"),
        ("CnNorthwest1", "cn-northwest-1"),
        ("EuNorth1", "eu-north-1"),
        ("EuCentral1", "eu-central-1"),
        ("EuCentral2", "eu-central-2"),
        ("EuWest1", "eu-west-1"),
        ("EuWest2", "eu-west-2"),
        ("EuWest3", "eu-west-3"),
        ("IlCentral1", "il-central-1"),
        ("MeSouth1", "me-south-1"),
        ("SaEast1", "sa-east-1"),
    ];
    for (legacy, expected) in mappings {
        let config: S3Config = serde_json::from_value(serde_json::json!({
            "bucket_name": "pkgly",
            "region": legacy,
            "credentials": {},
        }))
        .expect("legacy region should deserialize");
        assert_eq!(config.region.as_deref(), Some(expected));
    }
}

#[test]
fn unknown_region_round_trips_as_raw_string() {
    let config: S3Config = serde_json::from_value(serde_json::json!({
        "bucket_name": "pkgly",
        "region": "provider-special-1",
        "credentials": {},
    }))
    .expect("raw region should deserialize");
    let serialized = serde_json::to_value(config).expect("config should serialize");
    assert_eq!(serialized["region"], "provider-special-1");
}

#[test]
fn blank_region_is_rejected_without_custom_endpoint() {
    let config = S3Config {
        bucket_name: "pkgly".into(),
        region: Some("   ".into()),
        custom_region: None,
        credentials: S3Credentials::default(),
        path_style: true,
        cache: S3CacheConfig::default(),
        adaptive_buffer: AdaptiveBufferConfig::default(),
    };

    assert!(matches!(
        config.resolved_region(),
        Err(super::S3StorageError::NoRegionSpecified)
    ));
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

#[test]
fn empty_cache_path_uses_storage_default() {
    let config = S3CacheConfig {
        enabled: true,
        path: Some(std::path::PathBuf::new()),
        max_bytes: 8,
        max_entries: 4,
    };

    assert_eq!(
        resolve_cache_dir(&config, "blank-cache-path"),
        default_cache_dir("blank-cache-path")
    );
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

    let relative = {
        let state = cache.state.lock().await;
        state
            .entries
            .peek("first")
            .expect("first cache entry")
            .relative_path
            .clone()
    };
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

#[tokio::test]
async fn disk_cache_recovers_entries_and_preserves_unrelated_files() {
    let temp_dir = tempdir().expect("tempdir");
    let sentinel = temp_dir.path().join("sentinel.txt");
    fs::write(&sentinel, b"leave me alone")
        .await
        .expect("sentinel");

    let cache_config = S3CacheConfig {
        max_bytes: 64,
        ..cache_config_with_dir(temp_dir.path())
    };
    let cache = S3DiskCache::new(&cache_config, "test-cache")
        .await
        .expect("cache");
    cache
        .put(
            "recover",
            Bytes::from_static(b"persisted"),
            Some("text/plain"),
        )
        .await
        .expect("write cache entry");
    drop(cache);

    let recovered = S3DiskCache::new(&cache_config, "test-cache")
        .await
        .expect("recovered cache");
    let object = recovered
        .get("recover")
        .await
        .expect("cache read")
        .expect("recovered object");
    assert_eq!(object.bytes, Bytes::from_static(b"persisted"));
    assert_eq!(object.content_type.as_deref(), Some("text/plain"));
    assert_eq!(
        fs::read(&sentinel).await.expect("sentinel read"),
        b"leave me alone"
    );
}

#[tokio::test]
async fn disk_cache_recovers_latest_same_key_publication() {
    let temp_dir = tempdir().expect("tempdir");
    let cache_config = S3CacheConfig {
        max_bytes: 64,
        ..cache_config_with_dir(temp_dir.path())
    };
    let cache = S3DiskCache::new(&cache_config, "test-cache")
        .await
        .expect("cache");
    cache
        .put("replaced", Bytes::from_static(b"old"), Some("text/plain"))
        .await
        .expect("initial cache entry");
    cache
        .put(
            "replaced",
            Bytes::from_static(b"latest"),
            Some("text/plain"),
        )
        .await
        .expect("replacement cache entry");
    drop(cache);

    let recovered = S3DiskCache::new(&cache_config, "test-cache")
        .await
        .expect("recovered cache");
    let object = recovered
        .get("replaced")
        .await
        .expect("cache read")
        .expect("latest entry should recover");
    assert_eq!(object.bytes, Bytes::from_static(b"latest"));
}

#[tokio::test]
async fn disk_cache_recovery_preserves_object_timestamp() {
    let temp_dir = tempdir().expect("tempdir");
    let cache_config = S3CacheConfig {
        max_bytes: 64,
        ..cache_config_with_dir(temp_dir.path())
    };
    let cache = S3DiskCache::new(&cache_config, "test-cache")
        .await
        .expect("cache");
    let timestamp = FixedOffset::east_opt(0)
        .unwrap()
        .with_ymd_and_hms(2025, 1, 1, 0, 0, 0)
        .unwrap();
    cache
        .put_with_metadata(
            "timestamp",
            Bytes::from_static(b"payload"),
            Some("text/plain"),
            Some(timestamp),
        )
        .await
        .expect("write cache entry");
    drop(cache);

    let recovered = S3DiskCache::new(&cache_config, "test-cache")
        .await
        .expect("recovered cache");
    assert_eq!(
        recovered
            .get("timestamp")
            .await
            .expect("cache read")
            .expect("entry")
            .last_modified,
        Some(timestamp)
    );
}

#[tokio::test]
async fn disk_cache_discards_missing_or_malformed_sidecars() {
    let temp_dir = tempdir().expect("tempdir");
    let cache_config = S3CacheConfig {
        max_bytes: 64,
        ..cache_config_with_dir(temp_dir.path())
    };
    let cache = S3DiskCache::new(&cache_config, "test-cache")
        .await
        .expect("cache");
    cache
        .put("malformed", Bytes::from_static(b"payload"), None)
        .await
        .expect("malformed entry");
    cache
        .put("missing-sidecar", Bytes::from_static(b"payload"), None)
        .await
        .expect("missing sidecar entry");
    let (malformed_meta, missing_meta) = {
        let state = cache.state.lock().await;
        let malformed = state.entries.peek("malformed").expect("malformed entry");
        let missing = state
            .entries
            .peek("missing-sidecar")
            .expect("missing entry");
        (
            cache
                .dir
                .join(S3DiskCache::metadata_filename(&malformed.relative_path)),
            cache
                .dir
                .join(S3DiskCache::metadata_filename(&missing.relative_path)),
        )
    };
    fs::write(&malformed_meta, b"not json")
        .await
        .expect("malformed sidecar");
    fs::remove_file(&missing_meta)
        .await
        .expect("remove sidecar");
    drop(cache);

    let recovered = S3DiskCache::new(&cache_config, "test-cache")
        .await
        .expect("recovered cache");
    assert!(
        recovered
            .get("malformed")
            .await
            .expect("malformed read")
            .is_none()
    );
    assert!(
        recovered
            .get("missing-sidecar")
            .await
            .expect("missing read")
            .is_none()
    );
}

#[tokio::test]
async fn disk_cache_treats_corrupt_content_as_a_miss() {
    let temp_dir = tempdir().expect("tempdir");
    let cache_config = cache_config_with_dir(temp_dir.path());
    let cache = S3DiskCache::new(&cache_config, "test-cache")
        .await
        .expect("cache");
    cache
        .put("corrupt", Bytes::from_static(b"original"), None)
        .await
        .expect("write cache entry");

    let relative = {
        let state = cache.state.lock().await;
        state
            .entries
            .peek("corrupt")
            .expect("cache entry")
            .relative_path
            .clone()
    };
    fs::write(cache.dir.join(relative), b"tampered")
        .await
        .expect("corrupt cache entry");

    assert!(cache.get("corrupt").await.expect("cache read").is_none());
}

#[tokio::test]
async fn disk_cache_recovers_generation_and_removes_owned_orphans() {
    let temp_dir = tempdir().expect("tempdir");
    let cache_config = S3CacheConfig {
        max_bytes: 64,
        ..cache_config_with_dir(temp_dir.path())
    };
    let cache = S3DiskCache::new(&cache_config, "test-cache")
        .await
        .expect("cache");
    cache
        .put("kept", Bytes::from_static(b"kept"), None)
        .await
        .expect("write cache entry");
    let content_path = {
        let state = cache.state.lock().await;
        state
            .entries
            .peek("kept")
            .expect("entry")
            .relative_path
            .clone()
    };
    let parent = temp_dir.path().join(content_path.parent().expect("parent"));
    let base = S3DiskCache::hashed_filename("orphan");
    let orphan_generation = temp_dir
        .path()
        .join(base.parent().expect("base parent"))
        .join(format!(
            "{}.gen-abcdef",
            base.file_name().expect("base file").to_string_lossy()
        ));
    fs::create_dir_all(orphan_generation.parent().unwrap())
        .await
        .expect("orphan parent");
    fs::write(&orphan_generation, b"orphan")
        .await
        .expect("orphan generation");
    let legacy = temp_dir.path().join("aa").join("0".repeat(62));
    fs::create_dir_all(legacy.parent().unwrap())
        .await
        .expect("legacy parent");
    fs::write(&legacy, b"legacy")
        .await
        .expect("legacy artifact");
    let unrelated = parent.join("notes.txt");
    fs::write(&unrelated, b"keep this")
        .await
        .expect("unrelated");
    let unrelated_temp = parent.join("notes.tmp-abcdef");
    fs::write(&unrelated_temp, b"keep this too")
        .await
        .expect("unrelated temp");
    let unrelated_metadata = parent.join("notes.meta.json");
    fs::write(&unrelated_metadata, b"keep this metadata too")
        .await
        .expect("unrelated metadata");
    drop(cache);

    let recovered = S3DiskCache::new(&cache_config, "test-cache")
        .await
        .expect("reopen cache");
    assert!(recovered.get("kept").await.expect("cache read").is_some());
    assert!(
        !fs::try_exists(&orphan_generation)
            .await
            .expect("orphan exists")
    );
    assert!(!fs::try_exists(&legacy).await.expect("legacy exists"));
    assert!(fs::try_exists(&unrelated).await.expect("unrelated exists"));
    assert!(
        fs::try_exists(&unrelated_temp)
            .await
            .expect("unrelated temp exists")
    );
    assert!(
        fs::try_exists(&unrelated_metadata)
            .await
            .expect("unrelated metadata exists")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn disk_cache_rejects_symlinked_content() {
    use std::os::unix::fs::symlink;

    let temp_dir = tempdir().expect("tempdir");
    let outside = tempdir().expect("outside tempdir");
    let cache_config = S3CacheConfig {
        max_bytes: 64,
        ..cache_config_with_dir(temp_dir.path())
    };
    let cache = S3DiskCache::new(&cache_config, "test-cache")
        .await
        .expect("cache");
    cache
        .put("symlink", Bytes::from_static(b"payload"), None)
        .await
        .expect("write cache entry");
    let relative = {
        let state = cache.state.lock().await;
        state
            .entries
            .peek("symlink")
            .expect("entry")
            .relative_path
            .clone()
    };
    let content_path = cache.dir.join(relative);
    let target = outside.path().join("target");
    fs::write(&target, b"secret").await.expect("target");
    fs::remove_file(&content_path)
        .await
        .expect("remove content");
    symlink(&target, &content_path).expect("symlink");
    drop(cache);

    let recovered = S3DiskCache::new(&cache_config, "test-cache")
        .await
        .expect("reopen cache");
    assert!(
        recovered
            .get("symlink")
            .await
            .expect("cache read")
            .is_none()
    );
}

#[tokio::test]
async fn disk_cache_recovery_enforces_entry_capacity() {
    let temp_dir = tempdir().expect("tempdir");
    let write_config = S3CacheConfig {
        max_entries: 4,
        max_bytes: 64,
        ..cache_config_with_dir(temp_dir.path())
    };
    let cache = S3DiskCache::new(&write_config, "test-cache")
        .await
        .expect("cache");
    cache
        .put("old", Bytes::from_static(b"old"), None)
        .await
        .expect("old entry");
    sleep(Duration::from_millis(2)).await;
    cache
        .put("new", Bytes::from_static(b"new"), None)
        .await
        .expect("new entry");
    drop(cache);

    let read_config = S3CacheConfig {
        max_entries: 1,
        ..write_config
    };
    let recovered = S3DiskCache::new(&read_config, "test-cache")
        .await
        .expect("reopen cache");
    assert!(recovered.get("old").await.expect("old read").is_none());
    assert!(recovered.get("new").await.expect("new read").is_some());
}

#[tokio::test]
async fn disk_cache_does_not_insert_oversized_objects() {
    let temp_dir = tempdir().expect("tempdir");
    let config = S3CacheConfig {
        max_bytes: 3,
        ..cache_config_with_dir(temp_dir.path())
    };
    let cache = S3DiskCache::new(&config, "test-cache")
        .await
        .expect("cache");
    cache
        .put("too-large", Bytes::from_static(b"four"), None)
        .await
        .expect("oversized writes are ignored");
    assert!(cache.get("too-large").await.expect("cache read").is_none());
}

#[tokio::test]
async fn disk_cache_concurrent_publications_leave_a_valid_entry() {
    let temp_dir = tempdir().expect("tempdir");
    let config = S3CacheConfig {
        max_bytes: 128,
        ..cache_config_with_dir(temp_dir.path())
    };
    let cache = Arc::new(
        S3DiskCache::new(&config, "test-cache")
            .await
            .expect("cache"),
    );
    let mut tasks = Vec::new();
    for index in 0..8u8 {
        let cache = Arc::clone(&cache);
        tasks.push(tokio::spawn(async move {
            cache
                .put("same-key", Bytes::from(vec![index; 8]), None)
                .await
                .expect("concurrent publication");
        }));
    }
    for task in tasks {
        task.await.expect("publication task");
    }
    let object = cache
        .get("same-key")
        .await
        .expect("cache read")
        .expect("entry remains");
    assert_eq!(object.bytes.len(), 8);
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

#[test]
fn memory_snapshot_values_are_already_bytes() {
    let snapshot = MemorySnapshot::from_values(100, 25);
    assert_eq!(snapshot.total_bytes, 100);
    assert_eq!(snapshot.available_bytes, 25);
    assert!((snapshot.pressure() - 0.75).abs() < f64::EPSILON);
}

#[test]
fn memory_snapshot_cache_reuses_values_until_expiry() {
    let mut cache = MemorySnapshotCache::default();
    let start = tokio::time::Instant::now();
    let mut captures = 0;

    let first = cache.get_or_capture(start, || {
        captures += 1;
        Some(MemorySnapshot::from_values(100, 50))
    });
    let second = cache.get_or_capture(start + Duration::from_secs(1), || {
        captures += 1;
        Some(MemorySnapshot::from_values(200, 100))
    });
    let third = cache.get_or_capture(start + Duration::from_secs(6), || {
        captures += 1;
        Some(MemorySnapshot::from_values(200, 100))
    });

    assert_eq!(captures, 2);
    assert_eq!(first.expect("first").total_bytes, 100);
    assert_eq!(second.expect("second").total_bytes, 100);
    assert_eq!(third.expect("third").total_bytes, 200);
}

#[test]
fn memory_snapshot_cache_reuses_zero_memory_result_until_expiry() {
    let mut cache = MemorySnapshotCache::default();
    let start = tokio::time::Instant::now();
    let mut captures = 0;

    let first = cache.get_or_capture(start, || {
        captures += 1;
        None
    });
    let second = cache.get_or_capture(start + Duration::from_secs(1), || {
        captures += 1;
        Some(MemorySnapshot::from_values(100, 50))
    });

    assert!(first.is_none());
    assert!(second.is_none());
    assert_eq!(captures, 1);
}

#[test]
fn memory_limits_prefer_valid_lower_cgroup_limit() {
    assert_eq!(
        super::effective_memory_limits(1_000, 900, Some((400, 500))),
        (400, 400)
    );
    assert_eq!(
        super::effective_memory_limits(1_000, 900, Some((2_000, 100))),
        (1_000, 900)
    );
    assert_eq!(super::effective_memory_limits(0, 0, Some((0, 0))), (0, 0));
}

#[test]
fn zero_total_memory_pressure_is_safely_saturated() {
    let snapshot = MemorySnapshot::from_values(0, 0);
    assert_eq!(snapshot.pressure(), 1.0);
}

// ---------------------------------------------------------------------------
// delete_repository integration tests against a lightweight mock S3 endpoint.
// ---------------------------------------------------------------------------
use super::{S3Storage, S3StorageInner};
use crate::{FileContent, Storage, StorageConfigInner};
use aws_config::BehaviorVersion;
use aws_credential_types::{Credentials as AwsCredentials, provider::SharedCredentialsProvider};
use aws_sdk_s3::config::Builder as S3ConfigBuilder;
use aws_smithy_types::retry::RetryConfig;
use aws_types::region::Region;
use bytes::Buf;
use http_body_util::{BodyExt, Full};
use hyper::{
    Request, Response, StatusCode, body::Incoming, header, server::conn::http1, service::service_fn,
};
use hyper_util::rt::TokioIo;
use nr_core::storage::StoragePath;
use parking_lot::Mutex;
use std::{collections::VecDeque, convert::Infallible, net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use uuid::Uuid;

type RecordedBody = bytes::Bytes;
type Responder = Box<dyn FnMut(Request<RecordedBody>) -> ResponsePlan + Send + 'static>;

struct ResponsePlan {
    delay: Duration,
    response: Response<Full<bytes::Bytes>>,
}

impl ResponsePlan {
    fn immediate(response: Response<Full<bytes::Bytes>>) -> Self {
        Self {
            delay: Duration::ZERO,
            response,
        }
    }

    fn delayed(delay: Duration, response: Response<Full<bytes::Bytes>>) -> Self {
        Self { delay, response }
    }
}

#[derive(Clone, Debug)]
struct RecordedRequest {
    method: hyper::Method,
    uri: hyper::Uri,
    body: RecordedBody,
    headers: hyper::HeaderMap,
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
        headers: req_with_body.headers().clone(),
    });

    let plan = {
        let mut queue = responders.lock();
        let plan = if let Some(responder) = queue.front_mut() {
            responder(req_with_body)
        } else {
            ResponsePlan::immediate(
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Full::new(bytes::Bytes::from_static(
                        b"no responder for request",
                    )))
                    .unwrap(),
            )
        };
        // pop only after a successful response to preserve strict ordering on errors
        queue.pop_front();
        plan
    };
    if !plan.delay.is_zero() {
        sleep(plan.delay).await;
    }
    Ok(plan.response)
}

fn build_s3_storage(endpoint: &str, bucket: &str) -> S3Storage {
    let mut storage_config = StorageConfigInner::test_config();
    storage_config.storage_type = "s3".into();

    let client_config = S3ConfigBuilder::new()
        .region(Region::new("us-east-1"))
        .behavior_version(BehaviorVersion::latest())
        .force_path_style(true)
        .endpoint_url(endpoint)
        .retry_config(RetryConfig::standard().with_max_attempts(1))
        .credentials_provider(SharedCredentialsProvider::new(AwsCredentials::new(
            "AKIA", "SECRET", None, None, "mock",
        )))
        .build();

    let client = aws_sdk_s3::Client::from_conf(client_config);
    let config = S3Config {
        bucket_name: bucket.into(),
        region: Some("us-east-1".into()),
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
        manifest_cache: parking_lot::Mutex::new(super::ManifestCache::new()),
        manifest_load_lock: tokio::sync::Mutex::new(()),
    };

    S3Storage::from(inner)
}

fn build_s3_storage_with_cache(endpoint: &str, bucket: &str, cache: Arc<S3DiskCache>) -> S3Storage {
    let mut storage_config = StorageConfigInner::test_config();
    storage_config.storage_type = "s3".into();

    let client_config = S3ConfigBuilder::new()
        .region(Region::new("us-east-1"))
        .behavior_version(BehaviorVersion::latest())
        .force_path_style(true)
        .endpoint_url(endpoint)
        .retry_config(RetryConfig::standard().with_max_attempts(1))
        .credentials_provider(SharedCredentialsProvider::new(AwsCredentials::new(
            "AKIA", "SECRET", None, None, "mock",
        )))
        .build();

    let client = aws_sdk_s3::Client::from_conf(client_config);
    let config = S3Config {
        bucket_name: bucket.into(),
        region: Some("us-east-1".into()),
        custom_region: Some(CustomRegion {
            custom_region: Some("us-east-1".into()),
            endpoint: endpoint.parse().unwrap(),
        }),
        credentials: S3Credentials::new_access_key("AKIA", "SECRET"),
        path_style: true,
        cache: S3CacheConfig {
            enabled: true,
            path: Some(cache.dir.clone()),
            max_bytes: cache.max_bytes,
            max_entries: 4,
        },
        adaptive_buffer: AdaptiveBufferConfig::default(),
    };

    S3Storage::from(S3StorageInner {
        config,
        storage_config,
        client,
        cache: Some(cache),
        manifest_cache: parking_lot::Mutex::new(super::ManifestCache::new()),
        manifest_load_lock: tokio::sync::Mutex::new(()),
    })
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

fn list_response_body_with_metadata(
    prefix: &str,
    objects: &[(&str, u64, Option<&str>)],
    common_prefixes: &[&str],
    truncated: bool,
    token: Option<&str>,
) -> String {
    let contents = objects
        .iter()
        .map(|(key, size, last_modified)| {
            let last_modified = last_modified
                .map(|value| format!("<LastModified>{value}</LastModified>"))
                .unwrap_or_default();
            format!("<Contents><Key>{key}</Key>{last_modified}<Size>{size}</Size></Contents>")
        })
        .collect::<Vec<_>>()
        .join("");
    let prefixes = common_prefixes
        .iter()
        .map(|prefix| format!("<CommonPrefixes><Prefix>{prefix}</Prefix></CommonPrefixes>"))
        .collect::<Vec<_>>()
        .join("");
    let token_xml = token
        .map(|value| format!("<NextContinuationToken>{value}</NextContinuationToken>"))
        .unwrap_or_default();
    format!(
        r#"<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
    <Name>mock-bucket</Name>
    <Prefix>{prefix}</Prefix>
    <KeyCount>{}</KeyCount>
    <IsTruncated>{}</IsTruncated>
    {contents}
    {prefixes}
    {token_xml}
</ListBucketResult>"#,
        objects.len() + common_prefixes.len(),
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
        ResponsePlan::immediate(
            Response::builder()
                .status(StatusCode::OK)
                .body(Full::new(bytes::Bytes::from(body.clone())))
                .unwrap(),
        )
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
        ResponsePlan::immediate(delete_ok_response())
    })
}

fn respond_response(response: Response<Full<bytes::Bytes>>) -> Responder {
    Box::new(move |_| ResponsePlan::immediate(response.clone()))
}

fn respond_delayed(delay: Duration, response: Response<Full<bytes::Bytes>>) -> Responder {
    Box::new(move |_| ResponsePlan::delayed(delay, response.clone()))
}

fn response_with_body(
    status: StatusCode,
    body: impl Into<bytes::Bytes>,
) -> Response<Full<bytes::Bytes>> {
    Response::builder()
        .status(status)
        .body(Full::new(body.into()))
        .unwrap()
}

fn not_found_response(code: &str) -> Response<Full<bytes::Bytes>> {
    response_with_body(
        StatusCode::NOT_FOUND,
        bytes::Bytes::from(format!(
            r#"<Error><Code>{code}</Code><Message>missing</Message></Error>"#
        )),
    )
}

fn conditional_error_response(status: StatusCode, code: &str) -> Response<Full<bytes::Bytes>> {
    response_with_body(
        status,
        bytes::Bytes::from(format!(
            r#"<Error><Code>{code}</Code><Message>conditional write failed</Message></Error>"#
        )),
    )
}

fn head_response(
    status: StatusCode,
    etag: Option<&str>,
    size: Option<u64>,
    content_type: Option<&str>,
    last_modified: Option<&str>,
) -> Response<Full<bytes::Bytes>> {
    let mut builder = Response::builder().status(status);
    if let Some(etag) = etag {
        builder = builder.header(header::ETAG, etag);
    }
    if let Some(size) = size {
        builder = builder.header(header::CONTENT_LENGTH, size.to_string());
    }
    if let Some(content_type) = content_type {
        builder = builder.header(header::CONTENT_TYPE, content_type);
    }
    if let Some(last_modified) = last_modified {
        builder = builder.header(header::LAST_MODIFIED, last_modified);
    }
    builder.body(Full::new(bytes::Bytes::new())).unwrap()
}

fn get_response(
    body: &'static [u8],
    etag: Option<&str>,
    content_type: Option<&str>,
    last_modified: Option<&str>,
) -> Response<Full<bytes::Bytes>> {
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_LENGTH, body.len().to_string());
    if let Some(etag) = etag {
        builder = builder.header(header::ETAG, etag);
    }
    if let Some(content_type) = content_type {
        builder = builder.header(header::CONTENT_TYPE, content_type);
    }
    if let Some(last_modified) = last_modified {
        builder = builder.header(header::LAST_MODIFIED, last_modified);
    }
    builder
        .body(Full::new(bytes::Bytes::from_static(body)))
        .unwrap()
}

fn copy_response(etag: &str) -> Response<Full<bytes::Bytes>> {
    response_with_body(
        StatusCode::OK,
        bytes::Bytes::from(format!(
            r#"<CopyObjectResult><ETag>{etag}</ETag><LastModified>2025-01-01T00:00:00.000Z</LastModified></CopyObjectResult>"#
        )),
    )
}

fn multipart_create_response(upload_id: &str) -> Response<Full<bytes::Bytes>> {
    response_with_body(
        StatusCode::OK,
        bytes::Bytes::from(format!(
            r#"<InitiateMultipartUploadResult><UploadId>{upload_id}</UploadId></InitiateMultipartUploadResult>"#
        )),
    )
}

fn multipart_part_response(etag: &str) -> Response<Full<bytes::Bytes>> {
    response_with_body(
        StatusCode::OK,
        bytes::Bytes::from(format!(
            r#"<CopyPartResult><ETag>{etag}</ETag><LastModified>2025-01-01T00:00:00.000Z</LastModified></CopyPartResult>"#
        )),
    )
}

fn multipart_complete_response() -> Response<Full<bytes::Bytes>> {
    response_with_body(
        StatusCode::OK,
        bytes::Bytes::from_static(
            br#"<CompleteMultipartUploadResult><ETag>\"complete\"</ETag></CompleteMultipartUploadResult>"#,
        ),
    )
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
async fn delayed_responder_can_be_queued_for_timeout_tests() {
    let server = MockS3Server::start(vec![respond_delayed(
        Duration::from_millis(1),
        response_with_body(StatusCode::OK, bytes::Bytes::new()),
    )])
    .await;
    let mut stream = TcpStream::connect(server.address)
        .await
        .expect("connect delayed responder");
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("write request");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .expect("read response");
    assert!(response.starts_with(b"HTTP/1.1 200 OK"));
    server.shutdown().await;
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

#[tokio::test]
async fn append_uses_if_match_and_returns_appended_bytes() {
    let repository = Uuid::new_v4();
    let server = MockS3Server::start(vec![
        respond_response(get_response(
            b"old",
            Some("\"etag-old\""),
            Some("application/octet-stream"),
            None,
        )),
        respond_response(response_with_body(StatusCode::OK, bytes::Bytes::new())),
    ])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    let appended = storage
        .append_file(
            repository,
            FileContent::Bytes(Bytes::from_static(b"new")),
            &StoragePath::from("file.bin"),
        )
        .await
        .expect("append should succeed");

    assert_eq!(appended, 3);
    let requests = server.take_requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method, hyper::Method::GET);
    assert_eq!(requests[1].method, hyper::Method::PUT);
    assert_eq!(requests[1].body, Bytes::from_static(b"oldnew"));
    assert_eq!(
        requests[1].headers.get(header::IF_MATCH).unwrap(),
        "\"etag-old\""
    );
    assert!(requests[1].headers.get(header::IF_NONE_MATCH).is_none());
    server.shutdown().await;
}

#[tokio::test]
async fn append_uses_if_none_match_for_missing_object() {
    let repository = Uuid::new_v4();
    let server = MockS3Server::start(vec![
        respond_response(not_found_response("NoSuchKey")),
        respond_response(response_with_body(StatusCode::OK, bytes::Bytes::new())),
    ])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    let appended = storage
        .append_file(
            repository,
            FileContent::Content(b"new".to_vec()),
            &StoragePath::from("file.bin"),
        )
        .await
        .expect("append should create missing object");

    assert_eq!(appended, 3);
    let requests = server.take_requests();
    assert_eq!(requests[1].headers.get(header::IF_NONE_MATCH).unwrap(), "*");
    assert!(requests[1].headers.get(header::IF_MATCH).is_none());
    assert_eq!(requests[1].body, Bytes::from_static(b"new"));
    server.shutdown().await;
}

#[tokio::test]
async fn append_conflict_preserves_cache_and_returns_conflict() {
    let repository = Uuid::new_v4();
    let server = MockS3Server::start(vec![
        respond_response(get_response(b"old", Some("\"etag-old\""), None, None)),
        respond_response(conditional_error_response(
            StatusCode::PRECONDITION_FAILED,
            "PreconditionFailed",
        )),
    ])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    let error = storage
        .append_file(
            repository,
            FileContent::Bytes(Bytes::from_static(b"new")),
            &StoragePath::from("file.bin"),
        )
        .await
        .expect_err("stale ETag should produce a conflict");

    assert!(error.is_conflict());
    assert_eq!(server.take_requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn append_rejects_existing_object_without_etag() {
    let repository = Uuid::new_v4();
    let server = MockS3Server::start(vec![respond_response(get_response(
        b"old", None, None, None,
    ))])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    let error = storage
        .append_file(
            repository,
            FileContent::Content(b"new".to_vec()),
            &StoragePath::from("file.bin"),
        )
        .await
        .expect_err("append must not overwrite without an ETag");

    assert!(matches!(error.kind(), Some(super::S3ErrorKind::Other)));
    assert_eq!(server.take_requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn append_checks_concrete_ancestors_without_rechecking_final_target() {
    let repository = Uuid::new_v4();
    let server = MockS3Server::start(vec![
        respond_response(not_found_response("NotFound")),
        respond_response(get_response(b"old", Some("\"etag-old\""), None, None)),
        respond_response(response_with_body(StatusCode::OK, bytes::Bytes::new())),
    ])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    storage
        .append_file(
            repository,
            FileContent::Content(b"new".to_vec()),
            &StoragePath::from("parent/file.bin"),
        )
        .await
        .expect("append should succeed");
    let requests = server.take_requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].method, hyper::Method::HEAD);
    assert_eq!(requests[1].method, hyper::Method::GET);
    assert_eq!(requests[2].method, hyper::Method::PUT);
    server.shutdown().await;
}

#[tokio::test]
async fn save_rejects_concrete_object_ancestor() {
    let repository = Uuid::new_v4();
    let server = MockS3Server::start(vec![respond_response(head_response(
        StatusCode::OK,
        Some("\"parent\""),
        Some(1),
        None,
        None,
    ))])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    let error = storage
        .save_file(
            repository,
            FileContent::Content(b"new".to_vec()),
            &StoragePath::from("parent/file.bin"),
        )
        .await
        .expect_err("concrete parent objects must collide");
    assert!(matches!(error, super::S3StorageError::PathCollision(_)));
    assert_eq!(server.take_requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn get_file_information_paginates_virtual_directory_and_counts_all_entries() {
    let repository = Uuid::new_v4();
    let prefix = format!("{repository}/dir/");
    let server = MockS3Server::start(vec![
        respond_response(not_found_response("NotFound")),
        respond_list(list_response_body_with_metadata(
            &prefix,
            &[(
                &format!("{prefix}a.bin"),
                3,
                Some("2025-01-01T00:00:00.000Z"),
            )],
            &[&format!("{prefix}nested/")],
            true,
            Some("page-2"),
        )),
        respond_list(list_response_body_with_metadata(
            &prefix,
            &[(
                &format!("{prefix}b.bin"),
                4,
                Some("2025-01-02T00:00:00.000Z"),
            )],
            &[],
            false,
            None,
        )),
    ])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    let meta = storage
        .get_file_information(repository, &StoragePath::from("dir"))
        .await
        .expect("directory lookup should succeed")
        .expect("directory should exist");
    match meta.file_type {
        super::FileType::Directory(directory) => assert_eq!(directory.file_count, 3),
        other => panic!("expected directory metadata, got {other:?}"),
    }
    let requests = server.take_requests();
    assert_eq!(requests.len(), 3);
    assert!(
        requests[2]
            .uri
            .query()
            .unwrap_or_default()
            .contains("continuation-token=page-2")
    );
    server.shutdown().await;
}

#[tokio::test]
async fn virtual_directory_uses_placeholder_timestamp_when_available() {
    let repository = Uuid::new_v4();
    let prefix = format!("{repository}/dir/");
    let server = MockS3Server::start(vec![
        respond_response(not_found_response("NoSuchKey")),
        respond_list(list_response_body_with_metadata(
            &prefix,
            &[(&prefix, 0, Some("2025-01-04T00:00:00.000Z"))],
            &[],
            false,
            None,
        )),
    ])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    let meta = storage
        .get_file_information(repository, &StoragePath::from("dir"))
        .await
        .expect("directory lookup")
        .expect("directory")
        .modified;
    assert_eq!(meta.to_rfc3339(), "2025-01-04T00:00:00+00:00");
    server.shutdown().await;
}

#[tokio::test]
async fn object_timestamps_are_propagated_from_head_and_list() {
    let repository = Uuid::new_v4();
    let timestamp = "Wed, 01 Jan 2025 00:00:00 GMT";
    let server = MockS3Server::start(vec![respond_response(head_response(
        StatusCode::OK,
        Some("\"etag\""),
        Some(7),
        Some("text/plain"),
        Some(timestamp),
    ))])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    let meta = storage
        .get_file_information(repository, &StoragePath::from("file.txt"))
        .await
        .expect("head should succeed")
        .expect("file should exist");
    assert_eq!(meta.modified.to_rfc3339(), "2025-01-01T00:00:00+00:00");
    assert_eq!(meta.created, meta.modified);
    server.shutdown().await;
}

#[tokio::test]
async fn open_file_propagates_get_last_modified() {
    let repository = Uuid::new_v4();
    let server = MockS3Server::start(vec![respond_response(get_response(
        b"payload",
        Some("\"etag\""),
        Some("text/plain"),
        Some("Wed, 01 Jan 2025 00:00:00 GMT"),
    ))])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    let file = storage
        .open_file(repository, &StoragePath::from("file.txt"))
        .await
        .expect("get should succeed")
        .expect("file should exist");
    let (_, meta) = file.file().expect("file result");
    assert_eq!(meta.modified.to_rfc3339(), "2025-01-01T00:00:00+00:00");
    assert_eq!(meta.created, meta.modified);
    server.shutdown().await;
}

#[tokio::test]
async fn open_file_persists_request_timestamp_when_get_omits_last_modified() {
    let repository = Uuid::new_v4();
    let temp_dir = tempdir().expect("cache directory");
    let cache = Arc::new(
        S3DiskCache::new(
            &S3CacheConfig {
                max_bytes: 64,
                ..cache_config_with_dir(temp_dir.path())
            },
            "test-cache",
        )
        .await
        .expect("cache"),
    );
    let server = MockS3Server::start(vec![respond_response(get_response(
        b"payload",
        Some("\"etag\""),
        Some("text/plain"),
        None,
    ))])
    .await;
    let storage = build_s3_storage_with_cache(&server.endpoint(), "mock-bucket", cache);

    let first = storage
        .open_file(repository, &StoragePath::from("file.txt"))
        .await
        .expect("first get")
        .expect("file")
        .file()
        .expect("file result")
        .1
        .modified;
    let second = storage
        .open_file(repository, &StoragePath::from("file.txt"))
        .await
        .expect("cache get")
        .expect("cached file")
        .file()
        .expect("cached file result")
        .1
        .modified;

    assert_eq!(first, second);
    assert_eq!(server.take_requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn list_repository_objects_preserves_last_modified() {
    let repository = Uuid::new_v4();
    let prefix = format!("{repository}/");
    let server = MockS3Server::start(vec![respond_list(list_response_body_with_metadata(
        &prefix,
        &[(
            &format!("{prefix}manifest.json"),
            9,
            Some("2025-01-03T00:00:00.000Z"),
        )],
        &[],
        false,
        None,
    ))])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    let objects = storage
        .list_repository_objects(repository, None)
        .await
        .expect("list should succeed");
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].key, "manifest.json");
    assert_eq!(objects[0].size, 9);
    assert_eq!(
        objects[0].last_modified.unwrap().to_rfc3339(),
        "2025-01-03T00:00:00+00:00"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn control_timeout_is_classified_as_network() {
    let error = super::with_timeout(Duration::from_millis(5), async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok::<(), SmithySdkError<ErrorMetadata, SmithyHttpResponse>>(())
    })
    .await
    .expect_err("delayed control call should time out");
    assert!(matches!(error.kind(), Some(super::S3ErrorKind::Network)));
}

#[tokio::test]
async fn copy_move_uses_guarded_server_side_copy_without_get() {
    let repository = Uuid::new_v4();
    let source = StoragePath::from("folder/file name.bin");
    let destination = StoragePath::from("folder/moved.bin");
    let server = MockS3Server::start(vec![
        respond_response(head_response(
            StatusCode::OK,
            Some("\"source-etag\""),
            Some(12),
            Some("application/octet-stream"),
            None,
        )),
        respond_response(copy_response("\"destination-etag\"")),
        respond_response(response_with_body(
            StatusCode::NO_CONTENT,
            bytes::Bytes::new(),
        )),
    ])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock bucket");

    assert!(
        storage
            .move_file(repository, &source, &destination)
            .await
            .expect("move should succeed")
    );
    let requests = server.take_requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].method, hyper::Method::HEAD);
    assert_eq!(requests[1].method, hyper::Method::PUT);
    assert_eq!(requests[2].method, hyper::Method::DELETE);
    assert!(
        requests
            .iter()
            .all(|request| request.method != hyper::Method::GET)
    );
    assert_eq!(
        requests[1]
            .headers
            .get("x-amz-copy-source")
            .unwrap()
            .to_str()
            .unwrap(),
        format!("/mock%20bucket/{repository}%2Ffolder%2Ffile%20name.bin")
    );
    assert_eq!(
        requests[1]
            .headers
            .get("x-amz-copy-source-if-match")
            .unwrap(),
        "\"source-etag\""
    );
    assert_eq!(
        requests[2].headers.get(header::IF_MATCH).unwrap(),
        "\"source-etag\""
    );
    server.shutdown().await;
}

#[tokio::test]
async fn exactly_five_gib_uses_single_copy_object() {
    let repository = Uuid::new_v4();
    let server = MockS3Server::start(vec![
        respond_response(head_response(
            StatusCode::OK,
            Some("\"source-etag\""),
            Some(super::MULTIPART_COPY_THRESHOLD),
            None,
            None,
        )),
        respond_response(copy_response("\"destination-etag\"")),
        respond_response(response_with_body(
            StatusCode::NO_CONTENT,
            bytes::Bytes::new(),
        )),
    ])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    assert!(
        storage
            .move_file(
                repository,
                &StoragePath::from("source.bin"),
                &StoragePath::from("destination.bin"),
            )
            .await
            .expect("boundary move should succeed")
    );
    let requests = server.take_requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[1].method, hyper::Method::PUT);
    assert!(
        requests[1].uri.query().is_none() || !requests[1].uri.query().unwrap().contains("uploadId")
    );
    server.shutdown().await;
}

#[tokio::test]
async fn move_returns_false_for_missing_source() {
    let repository = Uuid::new_v4();
    let server = MockS3Server::start(vec![respond_response(not_found_response("NoSuchKey"))]).await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");
    assert!(
        !storage
            .move_file(
                repository,
                &StoragePath::from("source.bin"),
                &StoragePath::from("destination.bin"),
            )
            .await
            .expect("missing move should be idempotent")
    );
    assert_eq!(server.take_requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn move_copy_conflict_does_not_delete_source() {
    let repository = Uuid::new_v4();
    let server = MockS3Server::start(vec![
        respond_response(head_response(
            StatusCode::OK,
            Some("\"source-etag\""),
            Some(1),
            None,
            None,
        )),
        respond_response(conditional_error_response(
            StatusCode::PRECONDITION_FAILED,
            "PreconditionFailed",
        )),
    ])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");
    let error = storage
        .move_file(
            repository,
            &StoragePath::from("source.bin"),
            &StoragePath::from("destination.bin"),
        )
        .await
        .expect_err("copy conflict should be returned");
    assert!(error.is_conflict());
    let requests = server.take_requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|request| request.method != hyper::Method::DELETE)
    );
    server.shutdown().await;
}

#[tokio::test]
async fn move_delete_conflict_leaves_both_objects() {
    let repository = Uuid::new_v4();
    let server = MockS3Server::start(vec![
        respond_response(head_response(
            StatusCode::OK,
            Some("\"source-etag\""),
            Some(1),
            None,
            None,
        )),
        respond_response(copy_response("\"destination-etag\"")),
        respond_response(conditional_error_response(
            StatusCode::PRECONDITION_FAILED,
            "PreconditionFailed",
        )),
    ])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");
    let error = storage
        .move_file(
            repository,
            &StoragePath::from("source.bin"),
            &StoragePath::from("destination.bin"),
        )
        .await
        .expect_err("delete conflict should be returned");
    assert!(error.is_conflict());
    let requests = server.take_requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[2].headers.get(header::IF_MATCH).unwrap(),
        "\"source-etag\""
    );
    server.shutdown().await;
}

#[tokio::test]
async fn multipart_copy_aborts_after_failed_part() {
    let repository = Uuid::new_v4();
    let size = super::MULTIPART_COPY_THRESHOLD + 1;
    let server = MockS3Server::start(vec![
        respond_response(head_response(
            StatusCode::OK,
            Some("\"source-etag\""),
            Some(size),
            None,
            None,
        )),
        respond_response(multipart_create_response("upload-1")),
        respond_response(response_with_body(
            StatusCode::INTERNAL_SERVER_ERROR,
            bytes::Bytes::from_static(b"copy part failed"),
        )),
        respond_response(response_with_body(
            StatusCode::NO_CONTENT,
            bytes::Bytes::new(),
        )),
    ])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");
    let error = storage
        .move_file(
            repository,
            &StoragePath::from("source.bin"),
            &StoragePath::from("destination.bin"),
        )
        .await
        .expect_err("part failure should abort multipart copy");
    assert!(!error.is_conflict());
    let requests = server.take_requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[3].method, hyper::Method::DELETE);
    assert!(
        requests[3]
            .uri
            .query()
            .unwrap_or_default()
            .contains("uploadId=upload-1")
    );
    server.shutdown().await;
}

#[tokio::test]
async fn large_move_uses_multipart_copy_ranges_and_completes() {
    let repository = Uuid::new_v4();
    let size = super::MULTIPART_COPY_THRESHOLD + 1;
    let part_size = super::MULTIPART_COPY_PART_SIZE.max(size.div_ceil(10_000));
    let part_count = size.div_ceil(part_size);
    let mut source_head = head_response(
        StatusCode::OK,
        Some("\"source-etag\""),
        Some(size),
        Some("application/octet-stream"),
        None,
    );
    source_head
        .headers_mut()
        .insert(header::CACHE_CONTROL, "max-age=3600".parse().unwrap());
    source_head.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        "attachment; filename=artifact.bin".parse().unwrap(),
    );
    source_head
        .headers_mut()
        .insert(header::CONTENT_ENCODING, "gzip".parse().unwrap());
    source_head
        .headers_mut()
        .insert(header::CONTENT_LANGUAGE, "en".parse().unwrap());
    source_head.headers_mut().insert(
        header::EXPIRES,
        "Wed, 21 Oct 2037 07:28:00 GMT".parse().unwrap(),
    );
    source_head.headers_mut().insert(
        header::HeaderName::from_static("x-amz-website-redirect-location"),
        "/downloads/artifact.bin".parse().unwrap(),
    );
    source_head.headers_mut().insert(
        header::HeaderName::from_static("x-amz-meta-owner"),
        "pkgly".parse().unwrap(),
    );
    let mut responders = vec![
        respond_response(source_head),
        respond_response(multipart_create_response("upload-1")),
    ];
    for part in 0..part_count {
        responders.push(respond_response(multipart_part_response(&format!(
            "\"part-{part}\""
        ))));
    }
    responders.push(respond_response(multipart_complete_response()));
    responders.push(respond_response(response_with_body(
        StatusCode::NO_CONTENT,
        bytes::Bytes::new(),
    )));
    let server = MockS3Server::start(responders).await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    assert!(
        storage
            .move_file(
                repository,
                &StoragePath::from("source.bin"),
                &StoragePath::from("destination.bin"),
            )
            .await
            .expect("multipart move should succeed")
    );

    let requests = server.take_requests();
    assert_eq!(requests.len(), 2 + part_count as usize + 2);
    let create_headers = &requests[1].headers;
    assert_eq!(
        create_headers.get(header::CACHE_CONTROL).unwrap(),
        "max-age=3600"
    );
    assert_eq!(
        create_headers.get(header::CONTENT_DISPOSITION).unwrap(),
        "attachment; filename=artifact.bin"
    );
    assert_eq!(
        create_headers.get(header::CONTENT_ENCODING).unwrap(),
        "gzip"
    );
    assert_eq!(create_headers.get(header::CONTENT_LANGUAGE).unwrap(), "en");
    assert_eq!(
        create_headers.get(header::EXPIRES).unwrap(),
        "Wed, 21 Oct 2037 07:28:00 GMT"
    );
    assert_eq!(
        create_headers
            .get("x-amz-website-redirect-location")
            .unwrap(),
        "/downloads/artifact.bin"
    );
    assert_eq!(create_headers.get("x-amz-meta-owner").unwrap(), "pkgly");
    let part_requests = &requests[2..2 + part_count as usize];
    assert_eq!(
        part_requests[0]
            .headers
            .get("x-amz-copy-source-range")
            .unwrap(),
        "bytes=0-67108863"
    );
    assert!(
        part_requests
            .last()
            .unwrap()
            .headers
            .get("x-amz-copy-source-range")
            .is_some()
    );
    assert_eq!(
        requests[2 + part_count as usize].method,
        hyper::Method::POST
    );
    assert_eq!(requests.last().unwrap().method, hyper::Method::DELETE);
    server.shutdown().await;
}

#[tokio::test]
async fn manifest_pages_share_one_cached_traversal() {
    let repository = Uuid::new_v4();
    let server = MockS3Server::start(vec![respond_list(list_response_body_with_metadata(
        &format!("{repository}/v2/"),
        &[],
        &[],
        false,
        None,
    ))])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    let first = storage
        .list_docker_manifests_paginated(repository, 0, 10)
        .await
        .expect("first page");
    let second = storage
        .list_docker_manifests_paginated(repository, 10, 10)
        .await
        .expect("second page");
    assert!(first.0.is_empty());
    assert_eq!(first.1, 0);
    assert!(second.0.is_empty());
    assert_eq!(server.take_requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn concurrent_manifest_requests_share_one_traversal() {
    let repository = Uuid::new_v4();
    let server = MockS3Server::start(vec![respond_list(list_response_body_with_metadata(
        &format!("{repository}/v2/"),
        &[],
        &[],
        false,
        None,
    ))])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    let first_storage = storage.clone();
    let first = tokio::spawn(async move {
        first_storage
            .list_docker_manifests(repository)
            .await
            .expect("first manifest request")
    });
    let second_storage = storage.clone();
    let second = tokio::spawn(async move {
        second_storage
            .list_docker_manifests(repository)
            .await
            .expect("second manifest request")
    });
    assert!(first.await.expect("first task").is_empty());
    assert!(second.await.expect("second task").is_empty());
    assert_eq!(server.take_requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn mutation_during_manifest_load_prevents_stale_cache_publication() {
    let repository = Uuid::new_v4();
    let root = format!("{repository}/v2/");
    let server = MockS3Server::start(vec![
        respond_delayed(
            Duration::from_millis(75),
            response_with_body(
                StatusCode::OK,
                bytes::Bytes::from(list_response_body_with_metadata(
                    &root,
                    &[],
                    &[],
                    false,
                    None,
                )),
            ),
        ),
        respond_response(not_found_response("NotFound")),
        respond_response(response_with_body(StatusCode::OK, bytes::Bytes::new())),
        respond_list(list_response_body_with_metadata(
            &root,
            &[],
            &[],
            false,
            None,
        )),
    ])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    let loading_storage = storage.clone();
    let loading = tokio::spawn(async move {
        loading_storage
            .list_docker_manifests(repository)
            .await
            .expect("manifest traversal")
    });
    for _ in 0..100 {
        if !server.take_requests().is_empty() {
            break;
        }
        sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(
        server.take_requests().len(),
        1,
        "manifest load should start"
    );

    storage
        .save_file(
            repository,
            FileContent::Content(b"manifest".to_vec()),
            &StoragePath::from("manifest.json"),
        )
        .await
        .expect("save during traversal");
    assert!(loading.await.expect("manifest task").is_empty());

    storage
        .list_docker_manifests(repository)
        .await
        .expect("next read must traverse again");
    assert_eq!(server.take_requests().len(), 4);
    server.shutdown().await;
}

#[tokio::test]
async fn manifest_loader_sorts_nested_results_before_pagination() {
    let repository = Uuid::new_v4();
    let root = format!("{repository}/v2/");
    let image = format!("{repository}/v2/library/");
    let manifests = format!("{repository}/v2/library/image/manifests/");
    let server = MockS3Server::start(vec![
        respond_list(list_response_body_with_metadata(
            &root,
            &[],
            &[&image],
            false,
            None,
        )),
        respond_list(list_response_body_with_metadata(
            &image,
            &[],
            &[&manifests],
            false,
            None,
        )),
        respond_list(list_response_body_with_metadata(
            &manifests,
            &[
                (
                    &format!("{manifests}z"),
                    1,
                    Some("2025-01-01T00:00:00.000Z"),
                ),
                (
                    &format!("{manifests}a"),
                    2,
                    Some("2025-01-01T00:00:00.000Z"),
                ),
            ],
            &[],
            false,
            None,
        )),
    ])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    let (first, total) = storage
        .list_docker_manifests_paginated(repository, 0, 1)
        .await
        .expect("first manifest page");
    let (second, second_total) = storage
        .list_docker_manifests_paginated(repository, 1, 1)
        .await
        .expect("second manifest page");
    assert_eq!(total, 2);
    assert_eq!(second_total, 2);
    assert_eq!(first[0].key, "v2/library/image/manifests/a");
    assert_eq!(second[0].key, "v2/library/image/manifests/z");
    assert_eq!(server.take_requests().len(), 3);
    server.shutdown().await;
}

#[test]
fn manifest_cache_expires_and_invalidates_entries() {
    let repository = Uuid::new_v4();
    let now = tokio::time::Instant::now();
    let mut cache = super::ManifestCache::new();
    cache.insert(repository, Vec::new(), now);
    assert!(
        cache
            .get(repository, now + Duration::from_secs(29))
            .is_some()
    );
    assert!(
        cache
            .get(repository, now + Duration::from_secs(30))
            .is_none()
    );
    cache.insert(repository, Vec::new(), now);
    cache.invalidate(repository);
    assert!(cache.get(repository, now).is_none());
}

#[tokio::test]
async fn failed_manifest_traversal_is_not_cached() {
    let repository = Uuid::new_v4();
    let root = format!("{repository}/v2/");
    let server = MockS3Server::start(vec![
        respond_response(response_with_body(
            StatusCode::INTERNAL_SERVER_ERROR,
            bytes::Bytes::from_static(b"temporary failure"),
        )),
        respond_list(list_response_body_with_metadata(
            &root,
            &[],
            &[],
            false,
            None,
        )),
    ])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    assert!(storage.list_docker_manifests(repository).await.is_err());
    assert!(
        storage
            .list_docker_manifests(repository)
            .await
            .expect("retry should load")
            .is_empty()
    );
    assert_eq!(server.take_requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn successful_save_invalidates_manifest_cache() {
    let repository = Uuid::new_v4();
    let root = format!("{repository}/v2/");
    let server = MockS3Server::start(vec![
        respond_list(list_response_body_with_metadata(
            &root,
            &[],
            &[],
            false,
            None,
        )),
        respond_response(not_found_response("NotFound")),
        respond_response(response_with_body(StatusCode::OK, bytes::Bytes::new())),
        respond_list(list_response_body_with_metadata(
            &root,
            &[],
            &[],
            false,
            None,
        )),
    ])
    .await;
    let storage = build_s3_storage(&server.endpoint(), "mock-bucket");

    storage
        .list_docker_manifests(repository)
        .await
        .expect("initial traversal");
    storage
        .save_file(
            repository,
            FileContent::Content(b"manifest".to_vec()),
            &StoragePath::from("manifest.json"),
        )
        .await
        .expect("save should succeed");
    storage
        .list_docker_manifests(repository)
        .await
        .expect("traversal after invalidation");
    assert_eq!(server.take_requests().len(), 4);
    server.shutdown().await;
}
