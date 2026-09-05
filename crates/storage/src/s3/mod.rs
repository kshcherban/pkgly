// ABOUTME: Implements S3-backed repository storage and object metadata handling.
// ABOUTME: Provides guarded object mutation, listing, streaming, and local caching.
use std::{
    collections::VecDeque,
    env,
    future::Future,
    io::ErrorKind,
    net::IpAddr,
    num::NonZeroUsize,
    ops::Deref,
    path::PathBuf,
    pin::Pin,
    str::FromStr,
    sync::{Arc, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use aws_config::BehaviorVersion;
use aws_config::sts::AssumeRoleProvider;
use aws_credential_types::{Credentials as AwsCredentials, provider::SharedCredentialsProvider};
use aws_sdk_s3::{Client as AwsS3Client, types::CommonPrefix};
use aws_smithy_runtime_api::client::dns::{DnsFuture, ResolveDns, ResolveDnsError};
use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};
use aws_smithy_types::byte_stream::ByteStream;
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use aws_types::{SdkConfig, region::Region};
use bytes::{Bytes, BytesMut};
use chrono::{DateTime as ChronoDateTime, FixedOffset, Local, Utc};
use futures::future::BoxFuture;
use hex::encode;
use lru::LruCache;
use mime::Mime;
use nr_core::storage::{FileHashes, FileTypeCheck, SerdeMime, StoragePath};
use regions::CustomRegion;
use sha2::{Digest, Sha256};
use sysinfo::System;
use tokio::{
    fs,
    io::BufReader,
    sync::Mutex,
    task,
    time::{Duration, Instant},
};
use url::{Host, Url};

pub mod regions;
use ahash::HashSet;
use ipnet::IpNet;
use parking_lot::{Mutex as ParkingMutex, RwLock};
use serde::{Deserialize, Deserializer, Serialize};
use tracing::{debug, info, instrument, warn};
use utoipa::ToSchema;
use uuid::Uuid;
#[derive(Debug, thiserror::Error)]
pub enum S3StorageError {
    #[error("No Region Provided")]
    NoRegionSpecified,
    #[error("AWS SDK error ({kind:?}): {message}")]
    AwsSdkError { kind: S3ErrorKind, message: String },
    #[error("Bucket Does Not Exist {0}")]
    BucketDoesNotExist(String),
    #[error("IO Error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("Blocking task join error: {0}")]
    BlockingJoin(#[from] tokio::task::JoinError),
    #[error(transparent)]
    InvalidConfigType(#[from] InvalidConfigType),

    #[error(transparent)]
    PathCollision(#[from] PathCollisionError),
    #[error("S3 endpoint is blocked by egress policy")]
    BlockedEndpoint,
}

/// Broadly classifies S3 failures so callers can choose safe retry or conflict behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum S3ErrorKind {
    NotFound,
    AccessDenied,
    Throttled,
    Conflict,
    Network,
    Other,
}

impl S3StorageError {
    /// Returns the S3-specific classification when this is an AWS SDK failure.
    pub fn kind(&self) -> Option<S3ErrorKind> {
        match self {
            Self::AwsSdkError { kind, .. } => Some(*kind),
            _ => None,
        }
    }

    /// Returns whether the failure means the requested object or bucket was not found.
    pub fn is_not_found(&self) -> bool {
        self.kind() == Some(S3ErrorKind::NotFound)
    }

    /// Returns whether a conditional mutation failed because the object changed.
    pub fn is_conflict(&self) -> bool {
        self.kind() == Some(S3ErrorKind::Conflict)
    }

    /// Returns whether retrying may succeed without changing the request semantics.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.kind(),
            Some(S3ErrorKind::Network | S3ErrorKind::Throttled)
        )
    }

    /// Converts an SDK error into a classified failure while retaining its full display context.
    pub fn from_sdk_error<E>(err: SdkError<E, HttpResponse>) -> Self
    where
        E: std::error::Error + std::fmt::Display + ProvideErrorMetadata + 'static,
    {
        let kind = match &err {
            SdkError::TimeoutError(_) | SdkError::DispatchFailure(_) => S3ErrorKind::Network,
            SdkError::ServiceError(context) => {
                classify_service_error(context.err().code(), context.raw().status().as_u16())
            }
            SdkError::ConstructionFailure(_) | SdkError::ResponseError(_) => S3ErrorKind::Other,
            _ => S3ErrorKind::Other,
        };
        let message = format!(
            "{}",
            aws_smithy_types::error::display::DisplayErrorContext(&err)
        );
        Self::AwsSdkError { kind, message }
    }

    fn aws_message(message: impl Into<String>) -> Self {
        let message = message.into();
        Self::AwsSdkError {
            kind: classify_error_message(&message),
            message,
        }
    }
}

fn classify_service_error(code: Option<&str>, status: u16) -> S3ErrorKind {
    let code = code.unwrap_or_default().to_ascii_lowercase();
    if status == 409
        || status == 412
        || matches!(
            code.as_str(),
            "preconditionfailed" | "conditionalrequestconflict"
        )
    {
        return S3ErrorKind::Conflict;
    }
    if status == 404 || matches!(code.as_str(), "nosuchkey" | "nosuchbucket" | "notfound") {
        return S3ErrorKind::NotFound;
    }
    if status == 401 || status == 403 || code == "accessdenied" {
        return S3ErrorKind::AccessDenied;
    }
    if status == 429
        || status == 503
        || code.contains("throttl")
        || matches!(
            code.as_str(),
            "slowdown" | "requestlimitexceeded" | "toomanyrequests" | "serviceunavailable"
        )
    {
        return S3ErrorKind::Throttled;
    }
    S3ErrorKind::Other
}

fn classify_error_message(message: &str) -> S3ErrorKind {
    let lower = message.to_ascii_lowercase();
    if lower.contains("preconditionfailed")
        || lower.contains("precondition failed")
        || lower.contains("conditionalrequestconflict")
        || contains_status_code(&lower, 409)
        || contains_status_code(&lower, 412)
    {
        return S3ErrorKind::Conflict;
    }
    if lower.contains("nosuchkey")
        || lower.contains("no such key")
        || lower.contains("nosuchbucket")
        || lower.contains("no such bucket")
        || lower.contains("notfound")
        || lower.contains("not found")
        || contains_status_code(&lower, 404)
    {
        return S3ErrorKind::NotFound;
    }
    if lower.contains("accessdenied")
        || lower.contains("access denied")
        || contains_status_code(&lower, 401)
        || contains_status_code(&lower, 403)
    {
        return S3ErrorKind::AccessDenied;
    }
    if lower.contains("slowdown")
        || lower.contains("throttl")
        || lower.contains("requestlimitexceeded")
        || lower.contains("toomanyrequests")
        || lower.contains("serviceunavailable")
        || contains_status_code(&lower, 429)
        || contains_status_code(&lower, 503)
    {
        return S3ErrorKind::Throttled;
    }
    if lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("deadline")
        || lower.contains("dispatch")
        || lower.contains("connection")
        || lower.contains("network")
    {
        return S3ErrorKind::Network;
    }
    S3ErrorKind::Other
}

fn contains_status_code(message: &str, code: u16) -> bool {
    [
        format!("status code: {code}"),
        format!("status: {code}"),
        format!("status={code}"),
        format!("http {code}"),
        format!("http status {code}"),
    ]
    .iter()
    .any(|pattern| message.contains(pattern))
}

#[derive(Debug, Clone, Default)]
struct S3EgressPolicy {
    allowed_hosts: HashSet<String>,
    allowed_cidrs: Vec<IpNet>,
}

static S3_EGRESS_POLICY: OnceLock<RwLock<S3EgressPolicy>> = OnceLock::new();

pub fn install_egress_policy(
    allowed_hosts: &[String],
    allowed_cidrs: &[String],
) -> Result<(), String> {
    let policy = S3EgressPolicy {
        allowed_hosts: allowed_hosts
            .iter()
            .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
            .collect(),
        allowed_cidrs: allowed_cidrs
            .iter()
            .map(|cidr| cidr.parse().map_err(|_| cidr.clone()))
            .collect::<Result<_, _>>()?,
    };
    let lock = S3_EGRESS_POLICY.get_or_init(|| RwLock::new(policy.clone()));
    *lock.write() = policy;
    Ok(())
}

fn s3_egress_policy() -> S3EgressPolicy {
    let lock = S3_EGRESS_POLICY.get_or_init(|| RwLock::new(S3EgressPolicy::default()));
    lock.read().clone()
}

#[derive(Debug, Clone)]
struct S3DnsResolver;

impl ResolveDns for S3DnsResolver {
    fn resolve_dns<'a>(&'a self, name: &'a str) -> DnsFuture<'a> {
        let host = name.to_owned();
        let policy = s3_egress_policy();
        DnsFuture::new(async move {
            let addresses = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(ResolveDnsError::new)?
                .map(|address| address.ip())
                .collect::<Vec<_>>();
            if addresses.is_empty()
                || addresses.iter().any(|address| {
                    !nr_core::egress::is_global(*address)
                        && !policy.allowed_hosts.contains(&host.to_ascii_lowercase())
                        && !policy
                            .allowed_cidrs
                            .iter()
                            .any(|cidr| cidr.contains(address))
                })
            {
                return Err(ResolveDnsError::new(std::io::Error::other(
                    "S3 destination blocked by egress policy",
                )));
            }
            Ok(addresses)
        })
    }
}
use crate::{
    BorrowedStorageConfig, BorrowedStorageTypeConfig, DirectoryFileType, DynStorage, FileContent,
    FileContentBytes, FileFileType, FileType, InvalidConfigType, PathCollisionError,
    StaticStorageFactory, Storage, StorageConfig, StorageConfigInner, StorageError, StorageFactory,
    StorageFile, StorageFileMeta, StorageTypeConfig, StorageTypeConfigTrait,
    meta::RepositoryMeta,
    streaming::{DirectoryListStream, VecDirectoryListStream, collect_directory_stream},
    utils::new_type_arc_type,
};
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema, Default)]
pub struct S3Credentials {
    pub access_key: Option<String>,
    /// AWS secret key.
    pub secret_key: Option<String>,
    /// Session token for temporary credentials.
    pub session_token: Option<String>,
    /// Optional IAM role ARN to assume after establishing base credentials.
    pub role_arn: Option<String>,
    /// Explicit role session name override.
    pub role_session_name: Option<String>,
    /// External ID passed to STS when assuming a role.
    pub external_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticKeyCredentials {
    pub access_key: String,
    pub secret_key: String,
    pub session_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleAssumption {
    pub role_arn: String,
    pub session_name: Option<String>,
    pub external_id: Option<String>,
}

impl std::fmt::Debug for S3Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Credentials")
            .field("access_key", &self.access_key.as_ref().map(|_| "********")) // Mask access key
            .field("secret_key", &"********") // Always mask secret key
            .finish()
    }
}
impl S3Credentials {
    pub fn new_access_key(access_key: impl Into<String>, secret_key: impl Into<String>) -> Self {
        S3Credentials {
            access_key: Some(access_key.into()),
            secret_key: Some(secret_key.into()),
            session_token: None,
            role_arn: None,
            role_session_name: None,
            external_id: None,
        }
    }
    pub fn static_keys(&self) -> Option<StaticKeyCredentials> {
        let access_key = Self::clean_string(&self.access_key)?;
        let secret_key = Self::clean_string(&self.secret_key)?;
        Some(StaticKeyCredentials {
            access_key,
            secret_key,
            session_token: Self::clean_string(&self.session_token),
        })
    }

    pub fn role_to_assume(&self) -> Option<RoleAssumption> {
        let role_arn = Self::clean_string(&self.role_arn)?;
        Some(RoleAssumption {
            role_arn,
            session_name: Self::clean_string(&self.role_session_name),
            external_id: Self::clean_string(&self.external_id),
        })
    }

    fn clean_string(value: &Option<String>) -> Option<String> {
        value
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
            .map(|v| v.to_owned())
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct S3CacheConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    #[schema(value_type = String, format = "path")]
    pub path: Option<PathBuf>,
    #[serde(default = "default_cache_max_bytes")]
    pub max_bytes: u64,
    #[serde(default = "default_cache_entry_limit")]
    pub max_entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct AdaptiveBufferConfig {
    #[serde(default = "default_min_buffer_bytes")]
    pub min_buffer_bytes: u64,
    #[serde(default = "default_max_buffer_bytes")]
    pub max_buffer_bytes: u64,
    #[serde(default = "default_memory_pressure_threshold")]
    pub memory_pressure_threshold: f64,
}

impl Default for AdaptiveBufferConfig {
    fn default() -> Self {
        Self {
            min_buffer_bytes: default_min_buffer_bytes(),
            max_buffer_bytes: default_max_buffer_bytes(),
            memory_pressure_threshold: default_memory_pressure_threshold(),
        }
    }
}

fn default_min_buffer_bytes() -> u64 {
    DEFAULT_MIN_BUFFERED_OBJECT_BYTES
}

fn default_max_buffer_bytes() -> u64 {
    DEFAULT_MAX_BUFFERED_OBJECT_BYTES
}

fn default_memory_pressure_threshold() -> f64 {
    DEFAULT_MEMORY_PRESSURE_THRESHOLD
}

impl AdaptiveBufferConfig {
    fn buffer_limit_bytes(&self) -> u64 {
        if let Some(snapshot) = MemorySnapshot::capture() {
            return self.limit_for_pressure(snapshot.pressure());
        }
        self.bounds().1
    }

    fn bounds(&self) -> (u64, u64) {
        if self.min_buffer_bytes <= self.max_buffer_bytes {
            (self.min_buffer_bytes, self.max_buffer_bytes)
        } else {
            (self.max_buffer_bytes, self.min_buffer_bytes)
        }
    }

    pub(crate) fn limit_for_pressure(&self, pressure: f64) -> u64 {
        let pressure = pressure.clamp(0.0, 1.0);
        let (min_bytes, max_bytes) = self.bounds();
        if min_bytes == max_bytes {
            return min_bytes;
        }
        if self.memory_pressure_threshold <= 0.0 {
            return min_bytes;
        }
        if pressure >= self.memory_pressure_threshold {
            return min_bytes;
        }
        let span = max_bytes.saturating_sub(min_bytes) as f64;
        let ratio = pressure / self.memory_pressure_threshold;
        let remaining = 1.0 - ratio;
        let interpolated = min_bytes as f64 + span * remaining.clamp(0.0, 1.0);
        interpolated.round() as u64
    }
}

#[derive(Debug, Clone, Copy)]
struct MemorySnapshot {
    total_bytes: u64,
    available_bytes: u64,
}

#[derive(Debug, Default)]
struct MemorySnapshotCache {
    captured_at: Option<Instant>,
    snapshot: Option<MemorySnapshot>,
}

impl MemorySnapshotCache {
    fn get_or_capture<F>(&mut self, now: Instant, capture: F) -> Option<MemorySnapshot>
    where
        F: FnOnce() -> Option<MemorySnapshot>,
    {
        if let Some(captured_at) = self.captured_at
            && now
                .checked_duration_since(captured_at)
                .is_some_and(|elapsed| elapsed < MEMORY_SNAPSHOT_TTL)
        {
            return self.snapshot;
        }

        let snapshot = capture();
        self.captured_at = Some(now);
        self.snapshot = snapshot;
        snapshot
    }
}

static MEMORY_SNAPSHOT_CACHE: OnceLock<ParkingMutex<MemorySnapshotCache>> = OnceLock::new();

impl MemorySnapshot {
    #[cfg(test)]
    fn from_values(total_bytes: u64, available_bytes: u64) -> Self {
        Self {
            total_bytes,
            available_bytes: available_bytes.min(total_bytes),
        }
    }

    fn capture() -> Option<Self> {
        let cache =
            MEMORY_SNAPSHOT_CACHE.get_or_init(|| ParkingMutex::new(MemorySnapshotCache::default()));
        cache
            .lock()
            .get_or_capture(Instant::now(), Self::capture_uncached)
    }

    fn capture_uncached() -> Option<Self> {
        let mut system = System::new();
        system.refresh_memory();
        let host_total = system.total_memory();
        let host_available = system.available_memory();
        let cgroup = system
            .cgroup_limits()
            .map(|limits| (limits.total_memory, limits.free_memory));
        let (total, available) = effective_memory_limits(host_total, host_available, cgroup);
        if total == 0 {
            return None;
        }
        Some(Self {
            total_bytes: total,
            available_bytes: available,
        })
    }

    fn pressure(&self) -> f64 {
        if self.total_bytes == 0 {
            return 1.0;
        }
        let available_ratio = self.available_bytes as f64 / self.total_bytes as f64;
        (1.0 - available_ratio).clamp(0.0, 1.0)
    }
}

fn effective_memory_limits(
    host_total: u64,
    host_available: u64,
    cgroup: Option<(u64, u64)>,
) -> (u64, u64) {
    match cgroup {
        Some((total, available)) if total > 0 && total < host_total => {
            (total, available.min(total))
        }
        _ => (host_total, host_available.min(host_total)),
    }
}

impl Default for S3CacheConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            path: None,
            max_bytes: default_cache_max_bytes(),
            max_entries: default_cache_entry_limit(),
        }
    }
}

fn default_cache_max_bytes() -> u64 {
    512 * 1024 * 1024 // 512 MiB
}

fn default_cache_entry_limit() -> usize {
    2048
}

#[derive(Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct S3Config {
    pub bucket_name: String,
    #[serde(default, deserialize_with = "deserialize_region")]
    pub region: Option<String>,
    /// Custom region takes precedence over the region field
    #[serde(flatten)]
    pub custom_region: Option<CustomRegion>,
    pub credentials: S3Credentials,
    #[serde(default = "default_true")]
    #[schema(default = true)]
    pub path_style: bool,
    #[serde(default)]
    pub cache: S3CacheConfig,
    #[serde(default)]
    pub adaptive_buffer: AdaptiveBufferConfig,
}

fn deserialize_region<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    Ok(value.map(|region| legacy_region_id(&region).unwrap_or(region)))
}

fn legacy_region_id(value: &str) -> Option<String> {
    let region = match value {
        "UsEast1" => "us-east-1",
        "UsEast2" => "us-east-2",
        "UsWest1" => "us-west-1",
        "UsWest2" => "us-west-2",
        "CaCentral1" => "ca-central-1",
        "AfSouth1" => "af-south-1",
        "ApEast1" => "ap-east-1",
        "ApSouth1" => "ap-south-1",
        "ApNortheast1" => "ap-northeast-1",
        "ApNortheast2" => "ap-northeast-2",
        "ApNortheast3" => "ap-northeast-3",
        "ApSoutheast1" => "ap-southeast-1",
        "ApSoutheast2" => "ap-southeast-2",
        "CnNorth1" => "cn-north-1",
        "CnNorthwest1" => "cn-northwest-1",
        "EuNorth1" => "eu-north-1",
        "EuCentral1" => "eu-central-1",
        "EuCentral2" => "eu-central-2",
        "EuWest1" => "eu-west-1",
        "EuWest2" => "eu-west-2",
        "EuWest3" => "eu-west-3",
        "IlCentral1" => "il-central-1",
        "MeSouth1" => "me-south-1",
        "SaEast1" => "sa-east-1",
        _ => return None,
    };
    Some(region.to_owned())
}

impl std::fmt::Debug for S3Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Config")
            .field("bucket_name", &self.bucket_name)
            .field("region", &self.region)
            .field("custom_region", &self.custom_region)
            .field("credentials", &"********") // Mask credentials entirely
            .field("path_style", &self.path_style)
            .field("cache_enabled", &self.cache.enabled)
            .field("cache_path", &self.cache.path)
            .field("cache_max_bytes", &self.cache.max_bytes)
            .field("adaptive_buffer", &self.adaptive_buffer)
            .finish()
    }
}

fn default_true() -> bool {
    true
}
impl S3Config {
    pub fn resolved_region(&self) -> Result<Region, S3StorageError> {
        if let Some(custom) = &self.custom_region {
            if self.region.is_some() {
                warn!("Region set with custom region, custom region will take precedence");
            }
            let name = custom
                .custom_region
                .clone()
                .unwrap_or_else(|| "custom-endpoint".into());
            return Ok(Region::new(name));
        }
        if let Some(region) = self
            .region
            .as_deref()
            .map(str::trim)
            .filter(|region| !region.is_empty())
        {
            return Ok(Region::new(region.to_owned()));
        }
        Err(S3StorageError::NoRegionSpecified)
    }

    pub fn custom_endpoint(&self) -> Option<&Url> {
        self.custom_region.as_ref().map(|c| &c.endpoint)
    }

    pub fn cache_enabled(&self) -> bool {
        self.cache.enabled && self.cache.max_bytes > 0
    }
}
#[derive(Debug)]
pub(super) struct S3DiskCache {
    dir: PathBuf,
    max_bytes: u64,
    state: Mutex<CacheState>,
    publish_lock: Mutex<()>,
}

#[derive(Debug)]
struct CacheState {
    entries: LruCache<String, CacheEntry>,
    current_bytes: u64,
    failed_deletions: VecDeque<FailedDeletion>,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    relative_path: PathBuf,
    size: u64,
    digest: String,
    content_type: Option<String>,
    last_modified: Option<chrono::DateTime<FixedOffset>>,
}

#[derive(Debug, Clone)]
struct CachedObject {
    bytes: Bytes,
    content_type: Option<String>,
    last_modified: Option<chrono::DateTime<FixedOffset>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheMetadata {
    version: u8,
    key: String,
    relative_path: PathBuf,
    size: u64,
    digest: String,
    content_type: Option<String>,
    last_modified: Option<chrono::DateTime<FixedOffset>>,
    cached_at_ms: u128,
}

#[derive(Debug, Clone)]
struct FailedDeletion {
    relative_path: PathBuf,
    attempts: u32,
    next_retry: Instant,
}

impl FailedDeletion {
    fn new(relative_path: PathBuf) -> Self {
        Self {
            relative_path,
            attempts: 0,
            next_retry: Instant::now() + Duration::from_millis(FAILED_DELETION_BASE_DELAY_MS),
        }
    }

    fn ready(&self, now: Instant) -> bool {
        self.next_retry <= now
    }

    fn backoff(mut self) -> Self {
        self.attempts = self.attempts.saturating_add(1);
        let exponent = self.attempts.min(FAILED_DELETION_BACKOFF_CUTOFF);
        let multiplier = 1u64 << exponent;
        let delay_ms = FAILED_DELETION_BASE_DELAY_MS.saturating_mul(multiplier);
        let capped = delay_ms.min(FAILED_DELETION_MAX_DELAY_MS);
        self.next_retry = Instant::now() + Duration::from_millis(capped);
        self
    }
}

impl CacheState {
    fn push_failed_deletion(&mut self, entry: FailedDeletion) {
        if self.failed_deletions.len() >= FAILED_DELETION_QUEUE_LIMIT
            && let Some(dropped) = self.failed_deletions.pop_front()
        {
            warn!(
                path = %dropped.relative_path.display(),
                "Dropping oldest failed cache deletion to stay within bounds"
            );
        }
        self.failed_deletions.push_back(entry);
    }

    fn drain_due_failed_deletions(&mut self, now: Instant) -> Vec<FailedDeletion> {
        let mut due = Vec::new();
        while let Some(front) = self.failed_deletions.front() {
            if !front.ready(now) || due.len() >= FAILED_DELETION_MAX_RETRIES_PER_TICK {
                break;
            }
            if let Some(entry) = self.failed_deletions.pop_front() {
                due.push(entry);
            }
        }
        due
    }
}

impl S3DiskCache {
    async fn new(config: &S3CacheConfig, storage_name: &str) -> Result<Self, S3StorageError> {
        if config.max_bytes == 0 {
            return Err(S3StorageError::aws_message(
                "cache max_bytes must be greater than zero",
            ));
        }
        let dir = resolve_cache_dir(config, storage_name);
        fs::create_dir_all(&dir).await?;
        let capacity = NonZeroUsize::new(config.max_entries.max(1)).unwrap_or(NonZeroUsize::MIN);
        let entries = Self::recover_entries(&dir, capacity, config.max_bytes).await?;
        let current_bytes = entries.iter().map(|(_, entry)| entry.size).sum();
        let state = CacheState {
            entries,
            current_bytes,
            failed_deletions: VecDeque::new(),
        };
        Ok(Self {
            dir,
            max_bytes: config.max_bytes,
            state: Mutex::new(state),
            publish_lock: Mutex::new(()),
        })
    }

    fn hashed_filename(key: &str) -> PathBuf {
        let digest = Sha256::digest(key.as_bytes());
        let hex = encode(digest);
        let (prefix, rest) = hex.split_at(2);
        PathBuf::from(prefix).join(rest)
    }

    fn metadata_filename(relative: &std::path::Path) -> PathBuf {
        let Some(file_name) = relative.file_name().and_then(|name| name.to_str()) else {
            return relative.with_extension("meta.json");
        };
        let base_name = file_name
            .split_once(".gen-")
            .map_or(file_name, |(base, _)| base);
        relative
            .parent()
            .map(|parent| parent.join(format!("{base_name}.meta.json")))
            .unwrap_or_else(|| PathBuf::from(format!("{base_name}.meta.json")))
    }

    fn generated_filename(key: &str) -> PathBuf {
        let base = Self::hashed_filename(key);
        let Some(file_name) = base.file_name().and_then(|name| name.to_str()) else {
            return base;
        };
        base.parent()
            .map(|parent| parent.join(format!("{file_name}.gen-{}", Uuid::new_v4().simple())))
            .unwrap_or_else(|| {
                PathBuf::from(format!("{file_name}.gen-{}", Uuid::new_v4().simple()))
            })
    }

    fn is_owned_generation(name: &str) -> bool {
        let Some((base, generation)) = name.split_once(".gen-") else {
            return false;
        };
        base.len() == 62
            && base.chars().all(|ch| ch.is_ascii_hexdigit())
            && !generation.is_empty()
            && generation.chars().all(|ch| ch.is_ascii_hexdigit())
    }

    fn is_owned_legacy_content(name: &str) -> bool {
        name.len() == 62 && name.chars().all(|ch| ch.is_ascii_hexdigit())
    }

    fn is_owned_metadata(name: &str) -> bool {
        name.strip_suffix(".meta.json")
            .is_some_and(Self::is_owned_legacy_content)
    }

    fn is_owned_temp(name: &str) -> bool {
        let Some((base, suffix)) = name.split_once(".tmp-") else {
            return false;
        };
        if suffix.is_empty() || !suffix.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return false;
        }
        Self::is_owned_legacy_content(base)
            || base
                .strip_suffix(".meta")
                .is_some_and(Self::is_owned_legacy_content)
    }

    fn is_safe_relative_path(path: &std::path::Path) -> bool {
        !path.is_absolute()
            && path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
    }

    async fn recover_entries(
        dir: &std::path::Path,
        capacity: NonZeroUsize,
        max_bytes: u64,
    ) -> Result<LruCache<String, CacheEntry>, S3StorageError> {
        let mut recovered = Vec::new();
        let mut owned_prefixes = Vec::new();
        let mut valid_content = HashSet::default();
        let mut valid_metadata = HashSet::default();
        let mut prefixes = fs::read_dir(dir).await?;
        while let Some(prefix_entry) = prefixes.next_entry().await? {
            let prefix_path = prefix_entry.path();
            let prefix_name = prefix_entry.file_name();
            let prefix_name = prefix_name.to_string_lossy();
            if !prefix_entry.file_type().await?.is_dir()
                || prefix_name.len() != 2
                || !prefix_name.chars().all(|ch| ch.is_ascii_hexdigit())
            {
                continue;
            }
            owned_prefixes.push(prefix_path.clone());

            let mut files = fs::read_dir(&prefix_path).await?;
            while let Some(file_entry) = files.next_entry().await? {
                let file_name = file_entry.file_name().to_string_lossy().into_owned();
                if !Self::is_owned_metadata(&file_name) {
                    continue;
                }
                let metadata_path = file_entry.path();
                if !file_entry.file_type().await?.is_file() {
                    let _ = fs::remove_file(&metadata_path).await;
                    continue;
                }
                let relative_metadata = metadata_path
                    .strip_prefix(dir)
                    .map(PathBuf::from)
                    .map_err(|_| std::io::Error::other("cache metadata escaped root"))?;
                let metadata = fs::read(&metadata_path)
                    .await
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<CacheMetadata>(&bytes).ok());
                let Some(metadata) = metadata else {
                    let _ = fs::remove_file(&metadata_path).await;
                    continue;
                };
                let relative_content = metadata.relative_path.clone();
                let expected_content = Self::hashed_filename(&metadata.key);
                let expected_metadata = Self::metadata_filename(&expected_content);
                let valid_layout = Self::is_safe_relative_path(&relative_content)
                    && relative_content.parent() == expected_content.parent()
                    && relative_content
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| {
                            name.starts_with(
                                expected_content
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .unwrap_or_default(),
                            ) && Self::is_owned_generation(name)
                        });
                if metadata.version != CACHE_FORMAT_VERSION
                    || relative_metadata != expected_metadata
                    || !valid_layout
                    || metadata.size > max_bytes
                    || metadata.digest.len() != 64
                {
                    let _ = fs::remove_file(&metadata_path).await;
                    if valid_layout {
                        let _ = fs::remove_file(dir.join(&relative_content)).await;
                    }
                    continue;
                }

                let content_path = dir.join(&relative_content);
                let valid = if fs::symlink_metadata(&content_path)
                    .await
                    .map(|metadata| metadata.file_type().is_file())
                    .unwrap_or(false)
                {
                    match fs::read(&content_path).await {
                        Ok(bytes) => {
                            bytes.len() as u64 == metadata.size
                                && hex::encode(Sha256::digest(&bytes)) == metadata.digest
                        }
                        Err(_) => false,
                    }
                } else {
                    false
                };
                if !valid {
                    let _ = fs::remove_file(&metadata_path).await;
                    let _ = fs::remove_file(content_path).await;
                    continue;
                }

                valid_content.insert(relative_content.clone());
                valid_metadata.insert(relative_metadata);
                recovered.push((
                    metadata.cached_at_ms,
                    metadata.key,
                    CacheEntry {
                        relative_path: relative_content,
                        size: metadata.size,
                        digest: metadata.digest,
                        content_type: metadata.content_type,
                        last_modified: metadata.last_modified,
                    },
                ));
            }
        }

        // Remove only artifacts from the recognized cache layout. User files in the configured
        // directory, including files inside owned-looking prefixes, remain untouched.
        for prefix_path in owned_prefixes {
            let mut files = fs::read_dir(&prefix_path).await?;
            while let Some(file_entry) = files.next_entry().await? {
                let name = file_entry.file_name().to_string_lossy().into_owned();
                let relative = file_entry
                    .path()
                    .strip_prefix(dir)
                    .map(PathBuf::from)
                    .map_err(|_| std::io::Error::other("cache artifact escaped root"))?;
                let recognized = if Self::is_owned_metadata(&name) {
                    !valid_metadata.contains(&relative)
                } else if Self::is_owned_legacy_content(&name)
                    || Self::is_owned_generation(&name)
                    || Self::is_owned_temp(&name)
                {
                    !valid_content.contains(&relative)
                } else {
                    false
                };
                if recognized {
                    let _ = fs::remove_file(file_entry.path()).await;
                }
            }
        }

        recovered.sort_by_key(|(cached_at, _, _)| *cached_at);
        let mut entries: LruCache<String, CacheEntry> = LruCache::new(capacity);
        let mut current_bytes = 0u64;
        let mut evicted: Vec<PathBuf> = Vec::new();
        for (_, key, entry) in recovered {
            if let Some(old) = entries.pop(&key) {
                current_bytes = current_bytes.saturating_sub(old.size);
                evicted.push(old.relative_path);
            }
            if entries.len() >= capacity.get()
                && let Some((_, old)) = entries.pop_lru()
            {
                current_bytes = current_bytes.saturating_sub(old.size);
                evicted.push(old.relative_path);
            }
            entries.put(key, entry.clone());
            current_bytes = current_bytes.saturating_add(entry.size);
            while current_bytes > max_bytes {
                if let Some((_, old)) = entries.pop_lru() {
                    current_bytes = current_bytes.saturating_sub(old.size);
                    evicted.push(old.relative_path);
                } else {
                    break;
                }
            }
        }
        for relative in evicted {
            Self::remove_owned_files(dir, &relative).await;
        }
        Ok(entries)
    }

    async fn remove_owned_files(dir: &std::path::Path, relative: &std::path::Path) {
        let _ = fs::remove_file(dir.join(relative)).await;
        let _ = fs::remove_file(dir.join(Self::metadata_filename(relative))).await;
    }

    async fn get(&self, key: &str) -> Result<Option<CachedObject>, S3StorageError> {
        self.retry_failed_deletions().await;
        let entry = {
            let mut state = self.state.lock().await;
            match state.entries.get(key) {
                Some(entry) => entry.clone(),
                None => return Ok(None),
            }
        };
        let path = self.dir.join(&entry.relative_path);
        let is_regular_file = fs::symlink_metadata(&path)
            .await
            .map(|metadata| metadata.file_type().is_file())
            .unwrap_or(false);
        if !is_regular_file {
            self.remove_if_matches(key, &entry).await?;
            return Ok(None);
        }
        match fs::read(&path).await {
            Ok(data)
                if data.len() as u64 == entry.size
                    && hex::encode(Sha256::digest(&data)) == entry.digest =>
            {
                Ok(Some(CachedObject {
                    bytes: Bytes::from(data),
                    content_type: entry.content_type,
                    last_modified: entry.last_modified,
                }))
            }
            Ok(_) => {
                self.remove_if_matches(key, &entry).await?;
                Ok(None)
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                self.remove_if_matches(key, &entry).await?;
                Ok(None)
            }
            Err(err) => Err(err.into()),
        }
    }

    #[cfg(test)]
    async fn put(
        &self,
        key: &str,
        data: Bytes,
        content_type: Option<&str>,
    ) -> Result<(), S3StorageError> {
        self.put_with_metadata(key, data, content_type, None).await
    }

    async fn put_with_metadata(
        &self,
        key: &str,
        data: Bytes,
        content_type: Option<&str>,
        last_modified: Option<chrono::DateTime<FixedOffset>>,
    ) -> Result<(), S3StorageError> {
        self.retry_failed_deletions().await;
        if data.len() as u64 > self.max_bytes {
            self.remove(key).await?;
            return Ok(());
        }
        let relative = Self::generated_filename(key);
        let path = self.dir.join(&relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let temp_path = path.with_extension(format!("tmp-{}", Uuid::new_v4().simple()));
        fs::write(&temp_path, data.as_ref()).await?;
        let digest = hex::encode(Sha256::digest(data.as_ref()));
        let cached_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let metadata = CacheMetadata {
            version: CACHE_FORMAT_VERSION,
            key: key.to_owned(),
            relative_path: relative.clone(),
            size: data.len() as u64,
            digest: digest.clone(),
            content_type: content_type.map(str::to_owned),
            last_modified,
            cached_at_ms,
        };
        let metadata_relative = Self::metadata_filename(&relative);
        let metadata_path = self.dir.join(&metadata_relative);
        let metadata_temp =
            metadata_path.with_extension(format!("tmp-{}", Uuid::new_v4().simple()));
        let metadata_bytes = serde_json::to_vec(&metadata)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        fs::write(&metadata_temp, metadata_bytes).await?;
        let mut removed_entries = Vec::new();
        let mut superseded_generations = Vec::new();
        let _publication = self.publish_lock.lock().await;
        fs::rename(&temp_path, &path).await?;
        fs::rename(&metadata_temp, &metadata_path).await?;
        {
            let mut state = self.state.lock().await;
            if let Some(old) = state.entries.pop(key) {
                state.current_bytes = state.current_bytes.saturating_sub(old.size);
                if old.relative_path != relative {
                    superseded_generations.push(old.relative_path);
                }
            }
            if state.entries.len() >= state.entries.cap().get()
                && let Some((_, evicted)) = state.entries.pop_lru()
            {
                state.current_bytes = state.current_bytes.saturating_sub(evicted.size);
                removed_entries.push(evicted.relative_path);
            }
            state.entries.put(
                key.to_string(),
                CacheEntry {
                    relative_path: relative.clone(),
                    size: data.len() as u64,
                    digest,
                    content_type: content_type.map(str::to_owned),
                    last_modified,
                },
            );
            state.current_bytes = state.current_bytes.saturating_add(data.len() as u64);
            while state.current_bytes > self.max_bytes {
                if let Some((_, evicted)) = state.entries.pop_lru() {
                    state.current_bytes = state.current_bytes.saturating_sub(evicted.size);
                    removed_entries.push(evicted.relative_path);
                } else {
                    break;
                }
            }
        }
        for rel in superseded_generations {
            self.delete_content_path(rel, false).await;
        }
        for rel in removed_entries {
            self.delete_relative_path(rel).await;
        }
        Ok(())
    }

    async fn remove(&self, key: &str) -> Result<(), S3StorageError> {
        self.retry_failed_deletions().await;
        let removed = {
            let mut state = self.state.lock().await;
            state.entries.pop(key).map(|entry| {
                state.current_bytes = state.current_bytes.saturating_sub(entry.size);
                entry.relative_path
            })
        };
        if let Some(rel) = removed {
            self.delete_relative_path(rel).await;
        }
        Ok(())
    }

    async fn remove_if_matches(
        &self,
        key: &str,
        expected: &CacheEntry,
    ) -> Result<(), S3StorageError> {
        self.retry_failed_deletions().await;
        let removed = {
            let mut state = self.state.lock().await;
            let matches = state.entries.peek(key).is_some_and(|entry| {
                entry.relative_path == expected.relative_path && entry.digest == expected.digest
            });
            if matches {
                state.entries.pop(key).map(|entry| {
                    state.current_bytes = state.current_bytes.saturating_sub(entry.size);
                    entry.relative_path
                })
            } else {
                None
            }
        };
        if let Some(relative) = removed {
            self.delete_relative_path(relative).await;
        }
        Ok(())
    }

    async fn delete_relative_path(&self, relative: PathBuf) {
        self.delete_content_path(relative, true).await;
    }

    async fn delete_content_path(&self, relative: PathBuf, remove_metadata: bool) {
        let path = self.dir.join(&relative);
        let metadata_path = self.dir.join(Self::metadata_filename(&relative));
        match fs::remove_file(&path).await {
            Ok(_) => {
                debug!(path = %relative.display(), "Removed cache entry");
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                debug!(path = %relative.display(), "Cache entry already removed");
            }
            Err(err) => {
                warn!(
                    path = %relative.display(),
                    error = %err,
                    "Failed to delete cache entry; scheduling retry"
                );
                self.enqueue_failed_deletion(relative).await;
            }
        }
        if remove_metadata {
            let _ = fs::remove_file(metadata_path).await;
        }
    }

    async fn enqueue_failed_deletion(&self, relative_path: PathBuf) {
        let mut state = self.state.lock().await;
        state.push_failed_deletion(FailedDeletion::new(relative_path));
    }

    async fn retry_failed_deletions(&self) {
        let due = {
            let mut state = self.state.lock().await;
            state.drain_due_failed_deletions(Instant::now())
        };
        if due.is_empty() {
            return;
        }

        let mut still_pending = Vec::new();
        for mut entry in due {
            let path = self.dir.join(&entry.relative_path);
            match fs::remove_file(&path).await {
                Ok(_) => {
                    debug!(
                        path = %entry.relative_path.display(),
                        attempts = entry.attempts,
                        "Cache entry removed after retry"
                    );
                }
                Err(err) if err.kind() == ErrorKind::NotFound => {
                    debug!(
                        path = %entry.relative_path.display(),
                        "Cache entry already gone during retry"
                    );
                }
                Err(err) => {
                    warn!(
                        path = %entry.relative_path.display(),
                        attempts = entry.attempts + 1,
                        error = %err,
                        "Cache deletion retry failed"
                    );
                    entry = entry.backoff();
                    still_pending.push(entry);
                }
            }
        }

        if still_pending.is_empty() {
            return;
        }

        let mut state = self.state.lock().await;
        for entry in still_pending {
            state.push_failed_deletion(entry);
        }
    }
}

fn default_cache_dir(storage_name: &str) -> PathBuf {
    let sanitized = storage_name.replace('/', "_");
    env::temp_dir()
        .join("pkgly")
        .join("s3-cache")
        .join(sanitized)
}

fn resolve_cache_dir(config: &S3CacheConfig, storage_name: &str) -> PathBuf {
    config
        .path
        .as_ref()
        .filter(|path| !path.as_os_str().is_empty())
        .cloned()
        .unwrap_or_else(|| default_cache_dir(storage_name))
}

#[derive(Debug)]
pub struct S3StorageInner {
    pub config: S3Config,
    pub storage_config: StorageConfigInner,
    pub client: AwsS3Client,
    cache: Option<Arc<S3DiskCache>>,
    manifest_cache: ParkingMutex<ManifestCache>,
    manifest_load_lock: Mutex<()>,
}

#[derive(Debug)]
struct ManifestCache {
    entries: LruCache<Uuid, CachedManifestList>,
    generation: u64,
}

#[derive(Debug, Clone)]
struct CachedManifestList {
    items: Vec<S3ListedObject>,
    expires_at: Instant,
}

impl ManifestCache {
    fn new() -> Self {
        Self {
            entries: LruCache::new(NonZeroUsize::new(256).unwrap_or(NonZeroUsize::MIN)),
            generation: 0,
        }
    }

    fn get(&mut self, repository: Uuid, now: Instant) -> Option<Vec<S3ListedObject>> {
        let cached = self.entries.get(&repository)?;
        if cached.expires_at <= now {
            self.entries.pop(&repository);
            return None;
        }
        Some(cached.items.clone())
    }

    fn insert(&mut self, repository: Uuid, items: Vec<S3ListedObject>, now: Instant) {
        self.entries.put(
            repository,
            CachedManifestList {
                items,
                expires_at: now + MANIFEST_CACHE_TTL,
            },
        );
    }

    fn insert_if_generation(
        &mut self,
        repository: Uuid,
        items: Vec<S3ListedObject>,
        now: Instant,
        generation: u64,
    ) {
        if self.generation == generation {
            self.insert(repository, items, now);
        }
    }

    fn generation(&self) -> u64 {
        self.generation
    }

    fn invalidate(&mut self, repository: Uuid) {
        self.generation = self.generation.wrapping_add(1);
        self.entries.pop(&repository);
    }
}

const S3_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const S3_CONTROL_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(30);
const S3_CONTROL_TIMEOUT: Duration = Duration::from_secs(90);
const S3_COPY_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const S3_COPY_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const MULTIPART_COPY_THRESHOLD: u64 = 5 * 1024 * 1024 * 1024;
const MULTIPART_COPY_PART_SIZE: u64 = 64 * 1024 * 1024;

fn control_timeout_config() -> aws_smithy_types::timeout::TimeoutConfig {
    aws_smithy_types::timeout::TimeoutConfig::builder()
        .connect_timeout(S3_CONNECT_TIMEOUT)
        .read_timeout(S3_CONTROL_ATTEMPT_TIMEOUT)
        .operation_attempt_timeout(S3_CONTROL_ATTEMPT_TIMEOUT)
        .operation_timeout(S3_CONTROL_TIMEOUT)
        .build()
}

fn copy_timeout_config() -> aws_smithy_types::timeout::TimeoutConfig {
    aws_smithy_types::timeout::TimeoutConfig::builder()
        .connect_timeout(S3_CONNECT_TIMEOUT)
        .read_timeout(S3_CONTROL_ATTEMPT_TIMEOUT)
        .operation_attempt_timeout(S3_COPY_ATTEMPT_TIMEOUT)
        .operation_timeout(S3_COPY_TIMEOUT)
        .build()
}

fn streaming_timeout_config() -> aws_smithy_types::timeout::TimeoutConfig {
    aws_smithy_types::timeout::TimeoutConfig::builder()
        .connect_timeout(S3_CONNECT_TIMEOUT)
        .read_timeout(S3_CONTROL_ATTEMPT_TIMEOUT)
        .disable_operation_attempt_timeout()
        .disable_operation_timeout()
        .build()
}

fn timeout_override(
    timeout_config: aws_smithy_types::timeout::TimeoutConfig,
) -> aws_sdk_s3::config::Builder {
    aws_sdk_s3::config::Builder::new().timeout_config(timeout_config)
}

async fn with_timeout<T, E, F>(timeout_duration: Duration, future: F) -> Result<T, S3StorageError>
where
    F: Future<Output = Result<T, SdkError<E, HttpResponse>>>,
    E: std::error::Error + std::fmt::Display + ProvideErrorMetadata + 'static,
{
    match tokio::time::timeout(timeout_duration, future).await {
        Ok(result) => result.map_err(S3StorageError::from_sdk_error),
        Err(_) => Err(S3StorageError::aws_message(format!(
            "S3 request timed out after {} seconds",
            timeout_duration.as_secs()
        ))),
    }
}

async fn next_control_page<T, E, F>(
    deadline: Instant,
    future: F,
) -> Result<Option<T>, S3StorageError>
where
    F: Future<Output = Option<Result<T, SdkError<E, HttpResponse>>>>,
    E: std::error::Error + std::fmt::Display + ProvideErrorMetadata + 'static,
{
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(S3StorageError::aws_message(
            "S3 control operation exceeded its deadline",
        ));
    }
    let result = tokio::time::timeout(remaining, future)
        .await
        .map_err(|_| S3StorageError::aws_message("S3 LIST request timed out"))?;
    result
        .map(|page| page.map_err(S3StorageError::from_sdk_error))
        .transpose()
}

fn encode_copy_source_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

fn copy_source(bucket: &str, key: &str) -> String {
    format!(
        "/{}/{}",
        encode_copy_source_component(bucket),
        encode_copy_source_component(key)
    )
}

impl S3StorageInner {
    fn bucket(&self) -> &str {
        &self.config.bucket_name
    }
    fn aws_client(&self) -> &AwsS3Client {
        &self.client
    }
    pub async fn load_client(config: &S3Config) -> Result<AwsS3Client, S3StorageError> {
        let region = config.resolved_region()?;
        debug!(%region, bucket = %config.bucket_name, "Connecting to S3 bucket");

        let (base_config, static_provider) = build_base_config(config, &region).await?;

        let mut builder =
            aws_sdk_s3::config::Builder::from(&base_config).force_path_style(config.path_style);
        builder = builder.timeout_config(control_timeout_config());

        let http_client = aws_smithy_http_client::Builder::new()
            .tls_provider(aws_smithy_http_client::tls::Provider::rustls(
                aws_smithy_http_client::tls::rustls_provider::CryptoMode::Ring,
            ))
            .build_with_resolver(S3DnsResolver);
        builder = builder.http_client(http_client);

        if let Some(endpoint) = config.custom_endpoint() {
            let host = endpoint.host_str().ok_or(S3StorageError::BlockedEndpoint)?;
            let host_ip = match endpoint.host() {
                Some(Host::Ipv4(ip)) => Some(IpAddr::V4(ip)),
                Some(Host::Ipv6(ip)) => Some(IpAddr::V6(ip)),
                _ => None,
            };
            if !matches!(endpoint.scheme(), "http" | "https")
                || !endpoint.username().is_empty()
                || endpoint.password().is_some()
                || host_ip.is_some_and(|ip| {
                    !nr_core::egress::is_global(ip)
                        && !s3_egress_policy()
                            .allowed_hosts
                            .contains(&host.to_ascii_lowercase())
                        && !s3_egress_policy()
                            .allowed_cidrs
                            .iter()
                            .any(|cidr| cidr.contains(&ip))
                })
            {
                return Err(S3StorageError::BlockedEndpoint);
            }
            builder = builder.endpoint_url(endpoint.to_string());
        }

        if let Some(role) = config.credentials.role_to_assume() {
            let assume_provider = build_assume_role_provider(role, &base_config).await?;
            builder = builder.credentials_provider(SharedCredentialsProvider::new(assume_provider));
        } else if let Some(provider) = static_provider {
            builder = builder.credentials_provider(provider);
        }

        let client = AwsS3Client::from_conf(builder.build());
        match with_timeout(
            S3_CONTROL_TIMEOUT,
            client.head_bucket().bucket(&config.bucket_name).send(),
        )
        .await
        {
            Ok(_) => Ok(client),
            Err(error) if error.is_not_found() => Err(S3StorageError::BucketDoesNotExist(
                config.bucket_name.clone(),
            )),
            Err(error) => Err(error),
        }
    }

    pub(super) async fn build_cache(
        config: &S3Config,
        storage: &StorageConfigInner,
    ) -> Result<Option<Arc<S3DiskCache>>, S3StorageError> {
        if !config.cache_enabled() {
            return Ok(None);
        }
        let cache = S3DiskCache::new(&config.cache, &storage.storage_name).await?;
        Ok(Some(Arc::new(cache)))
    }
    pub fn s3_path(&self, repository: &Uuid, path: &StoragePath) -> String {
        format!("{}/{}", repository, path)
    }

    fn cache_key(&self, repository: &Uuid, path: &StoragePath) -> String {
        self.s3_path(repository, path)
    }

    fn should_cache(&self, path: &StoragePath) -> bool {
        self.cache.is_some() && !path.is_directory()
    }

    pub(super) fn meta_storage_path(location: &StoragePath) -> StoragePath {
        if location.is_directory() {
            let path = location.clone();
            path.push(".nr-meta")
        } else {
            let mut path_str = location.to_string();
            path_str.push_str(".nr-meta");
            StoragePath::from(path_str)
        }
    }

    /// Strip the repository UUID prefix from a full S3 key, returning the repository-relative
    /// path used by callers.
    pub(super) fn strip_repository_prefix<'a>(repository: &Uuid, key: &'a str) -> &'a str {
        let prefix = format!("{repository}/");
        if let Some(stripped) = key.strip_prefix(&prefix) {
            stripped
        } else {
            key
        }
    }

    pub(super) fn is_hidden_file(key: &str) -> bool {
        key.ends_with(".nr-meta") || key.split('/').any(|part| part == ".nr-meta")
    }

    async fn list_directory_entries(
        &self,
        prefix: &str,
    ) -> Result<(Vec<DirectoryObject>, Vec<String>), S3StorageError> {
        let mut paginator = self
            .aws_client()
            .list_objects_v2()
            .bucket(self.bucket())
            .prefix(prefix)
            .delimiter("/")
            .max_keys(1000)
            .into_paginator()
            .send();
        let deadline = Instant::now() + S3_CONTROL_TIMEOUT;
        let mut objects = Vec::new();
        let mut prefixes = Vec::new();
        while let Some(page) = next_control_page(deadline, paginator.next()).await? {
            objects.extend(page.contents().iter().filter_map(|object| {
                let key = object.key()?.to_owned();
                let size = object.size().unwrap_or_default().max(0) as u64;
                Some(DirectoryObject {
                    key,
                    size,
                    last_modified: s3_last_modified(object.last_modified()),
                })
            }));
            prefixes.extend(
                page.common_prefixes()
                    .iter()
                    .filter_map(CommonPrefix::prefix)
                    .map(str::to_owned),
            );
        }
        Ok((objects, prefixes))
    }

    async fn cache_get(
        &self,
        repository: &Uuid,
        location: &StoragePath,
    ) -> Result<Option<CachedObject>, S3StorageError> {
        if !self.should_cache(location) {
            return Ok(None);
        }
        let Some(cache) = &self.cache else {
            return Ok(None);
        };
        let key = self.cache_key(repository, location);
        cache.get(&key).await
    }

    async fn cache_put(
        &self,
        repository: &Uuid,
        location: &StoragePath,
        data: Bytes,
        content_type: Option<String>,
        last_modified: Option<ChronoDateTime<FixedOffset>>,
    ) -> Result<(), S3StorageError> {
        if !self.should_cache(location) {
            return Ok(());
        }
        if let Some(cache) = &self.cache {
            let key = self.cache_key(repository, location);
            let data_len = data.len();
            cache
                .put_with_metadata(&key, data, content_type.as_deref(), last_modified)
                .await?;
            debug!(
                repository = %repository,
                path = %location,
                bytes = data_len,
                "Cached S3 object locally"
            );
        }
        Ok(())
    }

    async fn cache_remove(
        &self,
        repository: &Uuid,
        location: &StoragePath,
    ) -> Result<(), S3StorageError> {
        if !self.should_cache(location) {
            return Ok(());
        }
        if let Some(cache) = &self.cache {
            let key = self.cache_key(repository, location);
            cache.remove(&key).await?;
        }
        Ok(())
    }

    async fn multipart_copy(
        &self,
        destination: &str,
        source: &str,
        source_etag: &str,
        object_size: u64,
        head: &aws_sdk_s3::operation::head_object::HeadObjectOutput,
    ) -> Result<(), S3StorageError> {
        let mut create = self
            .aws_client()
            .create_multipart_upload()
            .bucket(self.bucket())
            .key(destination);
        if let Some(cache_control) = head.cache_control() {
            create = create.cache_control(cache_control);
        }
        if let Some(content_disposition) = head.content_disposition() {
            create = create.content_disposition(content_disposition);
        }
        if let Some(content_encoding) = head.content_encoding() {
            create = create.content_encoding(content_encoding);
        }
        if let Some(content_language) = head.content_language() {
            create = create.content_language(content_language);
        }
        if let Some(content_type) = head.content_type() {
            create = create.content_type(content_type);
        }
        if let Some(expires) = head.expires_string().and_then(|value| {
            aws_smithy_types::DateTime::from_str(
                value,
                aws_smithy_types::date_time::Format::HttpDate,
            )
            .ok()
        }) {
            create = create.expires(expires);
        }
        if let Some(redirect) = head.website_redirect_location() {
            create = create.website_redirect_location(redirect);
        }
        if let Some(metadata) = head.metadata() {
            for (key, value) in metadata {
                create = create.metadata(key, value);
            }
        }
        let copy_deadline = Instant::now() + S3_COPY_TIMEOUT;
        let create_timeout = copy_deadline
            .saturating_duration_since(Instant::now())
            .min(S3_CONTROL_TIMEOUT);
        let created = with_timeout(create_timeout, create.send()).await?;
        let Some(upload_id) = created.upload_id() else {
            return Err(S3StorageError::aws_message(
                "S3 multipart copy did not return an upload ID",
            ));
        };
        let upload_id = upload_id.to_owned();
        let part_size = MULTIPART_COPY_PART_SIZE.max(object_size.div_ceil(10_000));
        let part_count = object_size.div_ceil(part_size);
        let mut completed_parts = Vec::with_capacity(part_count as usize);

        for part in 0..part_count {
            let start = part * part_size;
            let end = (start + part_size).min(object_size) - 1;
            let remaining = copy_deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.abort_multipart_copy(destination, &upload_id).await;
                return Err(S3StorageError::aws_message(
                    "S3 multipart copy exceeded its operation deadline",
                ));
            }
            let result = with_timeout(
                remaining,
                self.aws_client()
                    .upload_part_copy()
                    .bucket(self.bucket())
                    .key(destination)
                    .upload_id(&upload_id)
                    .part_number((part + 1) as i32)
                    .copy_source(source)
                    .copy_source_range(format!("bytes={start}-{end}"))
                    .copy_source_if_match(source_etag)
                    .customize()
                    .config_override(timeout_override(copy_timeout_config()))
                    .send(),
            )
            .await;
            let result = match result {
                Ok(result) => result,
                Err(error) => {
                    self.abort_multipart_copy(destination, &upload_id).await;
                    return Err(error);
                }
            };
            let Some(etag) = result.copy_part_result().and_then(|part| part.e_tag()) else {
                self.abort_multipart_copy(destination, &upload_id).await;
                return Err(S3StorageError::aws_message(
                    "S3 multipart copy part did not return an ETag",
                ));
            };
            completed_parts.push(
                aws_sdk_s3::types::CompletedPart::builder()
                    .part_number((part + 1) as i32)
                    .e_tag(etag)
                    .build(),
            );
        }

        let complete = self
            .aws_client()
            .complete_multipart_upload()
            .bucket(self.bucket())
            .key(destination)
            .upload_id(upload_id.clone())
            .multipart_upload(
                aws_sdk_s3::types::CompletedMultipartUpload::builder()
                    .set_parts(Some(completed_parts))
                    .build(),
            );
        let remaining = copy_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            self.abort_multipart_copy(destination, &upload_id).await;
            return Err(S3StorageError::aws_message(
                "S3 multipart copy exceeded its operation deadline",
            ));
        }
        if let Err(error) = with_timeout(remaining.min(S3_CONTROL_TIMEOUT), complete.send()).await {
            self.abort_multipart_copy(destination, &upload_id).await;
            return Err(error);
        }
        Ok(())
    }

    async fn abort_multipart_copy(&self, destination: &str, upload_id: &str) {
        let _ = with_timeout(
            S3_CONTROL_TIMEOUT,
            self.aws_client()
                .abort_multipart_upload()
                .bucket(self.bucket())
                .key(destination)
                .upload_id(upload_id)
                .send(),
        )
        .await;
    }

    fn invalidate_manifest_cache(&self, repository: Uuid) {
        self.manifest_cache.lock().invalidate(repository);
    }

    pub async fn get_path_for_creation(
        &self,
        repository: Uuid,
        location: &StoragePath,
    ) -> Result<String, S3StorageError> {
        let mut path = repository.to_string();
        let mut conflicting_path = StoragePath::default();
        let mut iter = location.clone().into_iter().peekable();

        while let Some(part) = iter.next() {
            path.push('/');
            path.push_str(part.as_ref());
            conflicting_path.push_mut(part.as_ref());

            let is_last = iter.peek().is_none();
            if !is_last && self.does_path_exist(&path).await? {
                // A parent segment is a concrete object, so we cannot place a child under it.
                return Err(PathCollisionError {
                    path: location.clone(),
                    conflicts_with: conflicting_path,
                }
                .into());
            }
        }

        Ok(path)
    }
    #[instrument]
    async fn does_path_exist(&self, path: &str) -> Result<bool, S3StorageError> {
        let result = tokio::time::timeout(
            S3_CONTROL_TIMEOUT,
            self.aws_client()
                .head_object()
                .bucket(self.bucket())
                .key(path)
                .send(),
        )
        .await
        .map_err(|_| S3StorageError::aws_message("S3 HEAD request timed out"))?;

        match result {
            Ok(_) => Ok(true),
            Err(SdkError::ServiceError(err)) if err.err().is_not_found() => Ok(false),
            Err(err) => Err(S3StorageError::from_sdk_error(err)),
        }
    }
    async fn get_directory_meta(
        &self,
        path: &str,
        modified: Option<ChronoDateTime<FixedOffset>>,
    ) -> Result<Option<StorageFileMeta<FileType>>, S3StorageError> {
        let file_file = FileType::Directory(DirectoryFileType { file_count: 0 });

        let name = path.split_once('/').map(|(_, rest)| rest).unwrap_or(path);
        let modified = modified.unwrap_or_else(|| Local::now().fixed_offset());
        let meta = StorageFileMeta {
            name: name.to_owned(),
            file_type: file_file,
            modified,
            created: modified,
        };

        Ok(Some(meta))
    }
}

async fn build_base_config(
    config: &S3Config,
    region: &Region,
) -> Result<(SdkConfig, Option<SharedCredentialsProvider>), S3StorageError> {
    let mut loader = aws_config::defaults(BehaviorVersion::latest()).region(region.clone());
    let mut static_provider = None;
    if let Some(keys) = config.credentials.static_keys() {
        let credentials = AwsCredentials::new(
            keys.access_key,
            keys.secret_key,
            keys.session_token,
            None,
            "pkgly-static",
        );
        let provider = SharedCredentialsProvider::new(credentials);
        loader = loader.credentials_provider(provider.clone());
        static_provider = Some(provider);
    }

    let shared_config = loader.load().await;
    Ok((shared_config, static_provider))
}

async fn build_assume_role_provider(
    role: RoleAssumption,
    base_config: &SdkConfig,
) -> Result<AssumeRoleProvider, S3StorageError> {
    let session_name = role.session_name.unwrap_or_else(default_session_name);
    let mut builder = AssumeRoleProvider::builder(role.role_arn).session_name(session_name);
    if let Some(external_id) = role.external_id {
        builder = builder.external_id(external_id);
    }
    let provider = builder.configure(base_config).build().await;
    Ok(provider)
}

fn default_session_name() -> String {
    format!("pkgly-{}", Uuid::new_v4().simple())
}

fn bytes_to_stream(bytes: Bytes) -> (ByteStream, usize) {
    let len = bytes.len();
    (ByteStream::from(bytes), len)
}

async fn file_into_bytes(file: FileContent) -> Result<Bytes, S3StorageError> {
    let bytes = task::spawn_blocking(move || FileContentBytes::try_from(file)).await??;
    Ok(match bytes {
        FileContentBytes::Content(content) => Bytes::from(content),
        FileContentBytes::Bytes(bytes) => bytes,
    })
}

async fn collect_body(stream: ByteStream) -> Result<Bytes, S3StorageError> {
    let aggregated = stream
        .collect()
        .await
        .map_err(|err| S3StorageError::aws_message(err.to_string()))?;
    Ok(aggregated.into_bytes())
}

const DEFAULT_MIN_BUFFERED_OBJECT_BYTES: u64 = 1024 * 1024; // 1 MiB
const DEFAULT_MAX_BUFFERED_OBJECT_BYTES: u64 = 8 * 1024 * 1024; // 8 MiB
const DEFAULT_MEMORY_PRESSURE_THRESHOLD: f64 = 0.75;
const MEMORY_SNAPSHOT_TTL: Duration = Duration::from_secs(5);
const CACHE_FORMAT_VERSION: u8 = 1;
const MANIFEST_CACHE_TTL: Duration = Duration::from_secs(30);
const FAILED_DELETION_QUEUE_LIMIT: usize = 1024;
const FAILED_DELETION_MAX_RETRIES_PER_TICK: usize = 64;
const FAILED_DELETION_BASE_DELAY_MS: u64 = 100;
const FAILED_DELETION_MAX_DELAY_MS: u64 = 30_000;
const FAILED_DELETION_BACKOFF_CUTOFF: u32 = 8;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum BodyRetrievalStrategy {
    BufferAndCache,
    StreamWithoutCache,
}

impl BodyRetrievalStrategy {
    fn from_content_length(
        length: Option<u64>,
        cache_enabled: bool,
        buffer_limit_bytes: u64,
    ) -> Self {
        if cache_enabled && length.is_some_and(|len| len <= buffer_limit_bytes) {
            return BodyRetrievalStrategy::BufferAndCache;
        }
        BodyRetrievalStrategy::StreamWithoutCache
    }

    fn should_cache(self) -> bool {
        matches!(self, BodyRetrievalStrategy::BufferAndCache)
    }
}

fn byte_stream_to_reader(stream: ByteStream) -> crate::StorageFileReader {
    let reader = BufReader::new(stream.into_async_read());
    let reader: Pin<Box<dyn tokio::io::AsyncRead + Send>> = Box::pin(reader);
    crate::StorageFileReader::AsyncReader(reader)
}

#[derive(Debug, Clone)]
pub struct S3ListedObject {
    /// Key relative to the repository root (e.g. `packages/pkg/file.tgz`).
    pub key: String,
    pub size: u64,
    /// Provider object timestamp, when the S3 response supplied a usable value.
    pub last_modified: Option<chrono::DateTime<FixedOffset>>,
}

fn s3_last_modified(
    value: Option<&aws_smithy_types::DateTime>,
) -> Option<ChronoDateTime<FixedOffset>> {
    let system_time: SystemTime = (*value?).try_into().ok()?;
    Some(ChronoDateTime::<Utc>::from(system_time).fixed_offset())
}

#[derive(Debug, Clone)]
struct DirectoryObject {
    key: String,
    size: u64,
    last_modified: Option<ChronoDateTime<FixedOffset>>,
}
#[derive(Debug, Clone)]
pub struct S3Storage(Arc<S3StorageInner>);
new_type_arc_type!(S3Storage(S3StorageInner));
impl Storage for S3Storage {
    type Error = S3StorageError;
    type DirectoryStream = VecDirectoryListStream;
    fn storage_type_name(&self) -> &'static str {
        "s3"
    }
    #[instrument(name = "Storage::unload", fields(storage_type = "s3"))]
    async fn unload(&self) -> Result<(), S3StorageError> {
        info!("Unloading S3 Storage");
        Ok(())
    }
    #[instrument(fields(storage_type = "s3"))]
    fn storage_config(&self) -> BorrowedStorageConfig<'_> {
        BorrowedStorageConfig {
            storage_config: &self.storage_config,
            config: BorrowedStorageTypeConfig::S3(&self.config),
        }
    }
    #[instrument(
        name = "Storage::save_file",
        fields(storage_type = "s3", repository = %repository, path = %location),
        skip(file)
    )]
    async fn save_file(
        &self,
        repository: uuid::Uuid,
        file: FileContent,
        location: &StoragePath,
    ) -> Result<(usize, bool), S3StorageError> {
        let path = self.get_path_for_creation(repository, location).await?;
        let already_exists = self.does_path_exist(&path).await?;
        if already_exists {
            debug!("File already exists, overwriting");
        }
        let content_type = if location.is_directory() {
            "application/x-directory"
        } else {
            "application/octet-stream"
        };
        let file_as_bytes = file_into_bytes(file).await?;
        let size = file_as_bytes.len();
        let cache_buffer = (self.should_cache(location)
            && size as u64 <= self.cache.as_ref().map_or(0, |cache| cache.max_bytes))
        .then(|| file_as_bytes.clone());
        let (body, _) = bytes_to_stream(file_as_bytes);
        self.aws_client()
            .put_object()
            .bucket(self.bucket())
            .key(&path)
            .body(body)
            .content_type(content_type)
            .customize()
            .config_override(timeout_override(streaming_timeout_config()))
            .send()
            .await
            .map_err(S3StorageError::from_sdk_error)?;
        debug!(path = %path, "File saved to S3");
        self.invalidate_manifest_cache(repository);
        let modified = Local::now().fixed_offset();
        if let Some(cache_buffer) = cache_buffer {
            self.cache_put(
                &repository,
                location,
                cache_buffer,
                Some(content_type.to_string()),
                Some(modified),
            )
            .await?;
        }
        Ok((size, !already_exists))
    }
    #[instrument(name = "Storage::append_file", fields(storage_type = "s3"))]
    async fn append_file(
        &self,
        repository: uuid::Uuid,
        file: FileContent,
        location: &StoragePath,
    ) -> Result<usize, S3StorageError> {
        // S3 doesn't support native append operations
        // We need to read, append, and write back
        // This is still O(n) for S3 since network I/O dominates
        let path = self.get_path_for_creation(repository, location).await?;

        let (current_bytes, current_etag) = match self
            .aws_client()
            .get_object()
            .bucket(self.bucket())
            .key(&path)
            .customize()
            .config_override(timeout_override(streaming_timeout_config()))
            .send()
            .await
        {
            Ok(response) => {
                let Some(etag) = response.e_tag().map(str::to_owned) else {
                    return Err(S3StorageError::aws_message(
                        "S3 existing object did not return an ETag; refusing an unguarded append",
                    ));
                };
                (collect_body(response.body).await?, Some(etag))
            }
            Err(error) => {
                let error = S3StorageError::from_sdk_error(error);
                if error.is_not_found() {
                    (Bytes::new(), None)
                } else {
                    return Err(error);
                }
            }
        };

        let appended = file_into_bytes(file).await?;
        let mut combined_buffer = BytesMut::with_capacity(current_bytes.len() + appended.len());
        combined_buffer.extend_from_slice(&current_bytes);
        combined_buffer.extend_from_slice(&appended);
        let combined_bytes = combined_buffer.freeze();

        let content_type = if location.is_directory() {
            "application/x-directory"
        } else {
            "application/octet-stream"
        };
        let mut request = self
            .aws_client()
            .put_object()
            .bucket(self.bucket())
            .key(&path)
            .content_type(content_type)
            .body(ByteStream::from(combined_bytes.clone()));
        request = match current_etag {
            Some(etag) => request.if_match(etag),
            None => request.if_none_match("*"),
        };
        request
            .customize()
            .config_override(timeout_override(streaming_timeout_config()))
            .send()
            .await
            .map_err(S3StorageError::from_sdk_error)?;
        let appended_size = appended.len();
        self.invalidate_manifest_cache(repository);
        let modified = Local::now().fixed_offset();
        if self.should_cache(location)
            && combined_bytes.len() as u64 <= self.cache.as_ref().map_or(0, |cache| cache.max_bytes)
        {
            self.cache_put(
                &repository,
                location,
                combined_bytes,
                Some(content_type.to_string()),
                Some(modified),
            )
            .await?;
        }
        Ok(appended_size)
    }
    #[instrument(name = "Storage::put_repository_meta", fields(storage_type = "s3"))]
    async fn put_repository_meta(
        &self,
        repository: uuid::Uuid,
        location: &StoragePath,
        value: RepositoryMeta,
    ) -> Result<(), S3StorageError> {
        let path = self.s3_path(&repository, location);

        if !location.is_directory() && !self.does_path_exist(&path).await? {
            return Err(S3StorageError::IOError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "File not found",
            )));
        } else if location.is_directory() {
            // For directories, ensure the prefix exists (or has been created) before attaching metadata.
            let prefix = if path.ends_with('/') {
                path.clone()
            } else {
                format!("{}/", path)
            };
            let probe = self
                .aws_client()
                .list_objects_v2()
                .bucket(self.bucket())
                .prefix(prefix)
                .max_keys(1);
            let probe = with_timeout(S3_CONTROL_TIMEOUT, probe.send()).await?;
            if probe.key_count().unwrap_or(0) == 0 {
                return Err(S3StorageError::IOError(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Directory not found",
                )));
            }
        }

        let meta_location = S3StorageInner::meta_storage_path(location);
        let meta_path = self.s3_path(&repository, &meta_location);
        let body = serde_json::to_vec(&value)
            .map(ByteStream::from)
            .map_err(|err| S3StorageError::IOError(std::io::Error::other(err)))?;

        with_timeout(
            S3_CONTROL_TIMEOUT,
            self.aws_client()
                .put_object()
                .bucket(self.bucket())
                .key(meta_path)
                .content_type("application/json")
                .body(body)
                .send(),
        )
        .await?;

        // Repository meta is small; we intentionally do not cache it to avoid polluting the blob cache.
        self.invalidate_manifest_cache(repository);
        Ok(())
    }
    #[instrument(name = "Storage::get_repository_meta", fields(storage_type = "s3"))]
    async fn get_repository_meta(
        &self,
        repository: uuid::Uuid,
        location: &StoragePath,
    ) -> Result<Option<RepositoryMeta>, S3StorageError> {
        let meta_location = S3StorageInner::meta_storage_path(location);
        let meta_path = self.s3_path(&repository, &meta_location);

        let response = match with_timeout(
            S3_CONTROL_TIMEOUT,
            self.aws_client()
                .get_object()
                .bucket(self.bucket())
                .key(&meta_path)
                .send(),
        )
        .await
        {
            Ok(resp) => resp,
            Err(error) if error.is_not_found() => return Ok(None),
            Err(error) => return Err(error),
        };

        let body = collect_body(response.body).await?;
        let meta: RepositoryMeta = serde_json::from_slice(&body).map_err(|err| {
            S3StorageError::IOError(std::io::Error::new(std::io::ErrorKind::InvalidData, err))
        })?;
        Ok(Some(meta))
    }
    #[instrument(
        name = "Storage::delete_file",
        fields(storage_type = "s3", repository = %repository, path = %location)
    )]
    async fn delete_file(
        &self,
        repository: uuid::Uuid,
        location: &StoragePath,
    ) -> Result<bool, S3StorageError> {
        let path = self.s3_path(&repository, location);
        let exists = self.does_path_exist(&path).await?;
        if !exists {
            return Ok(false);
        }
        with_timeout(
            S3_CONTROL_TIMEOUT,
            self.aws_client()
                .delete_object()
                .bucket(self.bucket())
                .key(&path)
                .send(),
        )
        .await?;
        self.cache_remove(&repository, location).await?;
        self.invalidate_manifest_cache(repository);
        Ok(true)
    }
    #[instrument(
        name = "Storage::move_file",
        fields(storage_type = "s3", repository = %repository, from = %from, to = %to),
        skip(self)
    )]
    async fn move_file(
        &self,
        repository: uuid::Uuid,
        from: &StoragePath,
        to: &StoragePath,
    ) -> Result<bool, S3StorageError> {
        let from_path = self.s3_path(&repository, from);
        let to_path = self.s3_path(&repository, to);

        let head = match with_timeout(
            S3_CONTROL_TIMEOUT,
            self.aws_client()
                .head_object()
                .bucket(self.bucket())
                .key(&from_path)
                .send(),
        )
        .await
        {
            Ok(head) => head,
            Err(error) if error.is_not_found() => {
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        let Some(source_etag) = head.e_tag().map(str::to_owned) else {
            return Err(S3StorageError::aws_message(
                "S3 source object did not return an ETag; refusing an unguarded move",
            ));
        };
        let object_size = head.content_length().unwrap_or_default().max(0) as u64;
        let source = copy_source(self.bucket(), &from_path);

        if object_size <= MULTIPART_COPY_THRESHOLD {
            with_timeout(
                S3_COPY_TIMEOUT,
                self.aws_client()
                    .copy_object()
                    .bucket(self.bucket())
                    .key(&to_path)
                    .copy_source(source.clone())
                    .copy_source_if_match(source_etag.clone())
                    .customize()
                    .config_override(timeout_override(copy_timeout_config()))
                    .send(),
            )
            .await?;
        } else {
            self.multipart_copy(&to_path, &source, &source_etag, object_size, &head)
                .await?;
        }

        // The destination changed as soon as the copy completed. Drop any stale destination
        // cache before attempting the guarded source deletion.
        self.cache_remove(&repository, to).await?;
        let delete_result = with_timeout(
            S3_CONTROL_TIMEOUT,
            self.aws_client()
                .delete_object()
                .bucket(self.bucket())
                .key(&from_path)
                .if_match(source_etag)
                .send(),
        )
        .await;
        if let Err(error) = delete_result {
            // A failed conditional delete may mean the source changed or that the request was
            // accepted before the connection failed; both cache entries are unsafe to retain.
            self.cache_remove(&repository, from).await?;
            self.invalidate_manifest_cache(repository);
            return Err(error);
        }

        self.cache_remove(&repository, from).await?;
        self.invalidate_manifest_cache(repository);
        Ok(true)
    }
    #[instrument(
        name = "Storage::get_file_information",
        fields(storage_type = "s3", repository = %repository, path = %location),
        skip(self)
    )]
    async fn get_file_information(
        &self,
        repository: uuid::Uuid,
        location: &StoragePath,
    ) -> Result<Option<crate::StorageFileMeta<FileType>>, S3StorageError> {
        if let Some(cached) = self.cache_get(&repository, location).await? {
            let mime_type = cached
                .content_type
                .as_deref()
                .and_then(|ct| Mime::from_str(ct).ok())
                .map(SerdeMime);
            let size = cached.bytes.len() as u64;
            let modified = cached
                .last_modified
                .unwrap_or_else(|| Local::now().fixed_offset());
            return Ok(Some(StorageFileMeta::<FileType> {
                name: location.to_string(),
                file_type: FileType::File(FileFileType {
                    file_size: size,
                    mime_type,
                    file_hash: FileHashes::default(),
                }),
                modified,
                created: modified,
            }));
        }

        let path = self.s3_path(&repository, location);
        let head = match with_timeout(
            S3_CONTROL_TIMEOUT,
            self.aws_client()
                .head_object()
                .bucket(self.bucket())
                .key(&path)
                .send(),
        )
        .await
        {
            Ok(head) => head,
            Err(error) if error.is_not_found() => {
                // Maybe this is a directory prefix without a placeholder object.
                let prefix = if path.ends_with('/') {
                    path.clone()
                } else {
                    format!("{}/", path)
                };
                let (objects, prefixes) = self.list_directory_entries(&prefix).await?;
                if objects.is_empty() && prefixes.is_empty() {
                    return Ok(None);
                }

                let count = objects
                    .iter()
                    .filter(|object| {
                        object.key != prefix && !S3StorageInner::is_hidden_file(&object.key)
                    })
                    .count()
                    + prefixes
                        .iter()
                        .filter(|prefix| !S3StorageInner::is_hidden_file(prefix))
                        .count();
                let modified = objects
                    .iter()
                    .find(|object| object.key == prefix)
                    .and_then(|object| object.last_modified)
                    .unwrap_or_else(|| Local::now().fixed_offset());

                let dir_meta = StorageFileMeta::<FileType> {
                    name: location.to_string(),
                    file_type: FileType::Directory(DirectoryFileType {
                        file_count: count as u64,
                    }),
                    modified,
                    created: modified,
                };
                return Ok(Some(dir_meta));
            }
            Err(error) => return Err(error),
        };

        let content_type = head.content_type().map(|ct| ct.to_string());
        if content_type
            .as_deref()
            .is_some_and(|ct| ct == "application/x-directory")
            && let Some(meta) = self
                .get_directory_meta(&path, s3_last_modified(head.last_modified()))
                .await?
        {
            return Ok(Some(meta));
        }

        let file_size: u64 = head
            .content_length()
            .unwrap_or_default()
            .try_into()
            .unwrap_or_default();
        let mime_type = content_type
            .as_deref()
            .map(Mime::from_str)
            .transpose()
            .unwrap_or_default()
            .map(SerdeMime);

        let modified =
            s3_last_modified(head.last_modified()).unwrap_or_else(|| Local::now().fixed_offset());

        let meta = StorageFileMeta::<FileType> {
            name: location.to_string(),
            file_type: FileType::File(FileFileType {
                file_size,
                mime_type,
                file_hash: FileHashes::default(),
            }),
            modified,
            created: modified,
        };
        Ok(Some(meta))
    }
    #[instrument(
        name = "Storage::open_file",
        fields(storage_type = "s3", repository = %repository, location = %location)
    )]
    async fn open_file(
        &self,
        repository: uuid::Uuid,
        location: &StoragePath,
    ) -> Result<Option<crate::StorageFile>, S3StorageError> {
        if let Some(cached) = self.cache_get(&repository, location).await? {
            let mime_type = cached
                .content_type
                .as_deref()
                .and_then(|ct| Mime::from_str(ct).ok())
                .map(SerdeMime);
            let size = cached.bytes.len() as u64;
            let modified = cached
                .last_modified
                .unwrap_or_else(|| Local::now().fixed_offset());
            let meta = StorageFileMeta::<FileFileType> {
                name: location.to_string(),
                file_type: FileFileType {
                    file_size: size,
                    mime_type,
                    file_hash: FileHashes::default(),
                },
                modified,
                created: modified,
            };
            let result = StorageFile::File {
                meta,
                content: crate::StorageFileReader::Bytes(FileContentBytes::Bytes(cached.bytes)),
            };
            return Ok(Some(result));
        }
        let path = self.s3_path(&repository, location);
        let response = match self
            .aws_client()
            .get_object()
            .bucket(self.bucket())
            .key(&path)
            .customize()
            .config_override(timeout_override(streaming_timeout_config()))
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(SdkError::ServiceError(err)) if err.err().is_no_such_key() => {
                return self.collect_directory(repository, location, None).await;
            }
            Err(err) => return Err(S3StorageError::from_sdk_error(err)),
        };

        let response_content_type = response.content_type().map(|ct| ct.to_string());
        let response_last_modified = s3_last_modified(response.last_modified());
        if response_content_type
            .as_deref()
            .map(|ct| ct == "application/x-directory")
            .unwrap_or(false)
        {
            return self
                .collect_directory(repository, location, response_last_modified)
                .await;
        }
        let response_length_opt = response
            .content_length()
            .and_then(|len| len.try_into().ok());
        let response_length = response_length_opt.unwrap_or_default();
        let cache_allowed = self.should_cache(location);
        let buffer_limit = self.config.adaptive_buffer.buffer_limit_bytes();
        let strategy = BodyRetrievalStrategy::from_content_length(
            response_length_opt,
            cache_allowed,
            buffer_limit,
        );
        debug!(
            repository = %repository,
            path = %path,
            object_size = response_length,
            streaming = matches!(strategy, BodyRetrievalStrategy::StreamWithoutCache),
            cache_allowed,
            "Fetched object from S3"
        );
        if cache_allowed && !strategy.should_cache() {
            debug!(
                repository = %repository,
                path = %path,
                object_size = response_length,
                threshold = buffer_limit,
                "Skipping cache write for oversized S3 object"
            );
        }

        let modified = response_last_modified.unwrap_or_else(|| Local::now().fixed_offset());
        let meta = StorageFileMeta::<FileFileType> {
            name: location.to_string(),
            file_type: FileFileType {
                file_size: response_length,
                mime_type: response_content_type
                    .as_deref()
                    .map(Mime::from_str)
                    .transpose()
                    .unwrap_or_default()
                    .map(SerdeMime),
                file_hash: FileHashes::default(),
            },
            modified,
            created: modified,
        };
        let content = match strategy {
            BodyRetrievalStrategy::BufferAndCache => {
                let body = collect_body(response.body).await?;
                if strategy.should_cache() {
                    self.cache_put(
                        &repository,
                        location,
                        body.clone(),
                        response_content_type.clone(),
                        Some(modified),
                    )
                    .await?;
                }
                crate::StorageFileReader::Bytes(FileContentBytes::Bytes(body))
            }
            BodyRetrievalStrategy::StreamWithoutCache => byte_stream_to_reader(response.body),
        };
        let result = StorageFile::File { meta, content };

        Ok(Some(result))
    }

    #[instrument(name = "Storage::validate_config_change", fields(storage_type = "s3"))]
    async fn validate_config_change(
        &self,
        config: StorageTypeConfig,
    ) -> Result<(), S3StorageError> {
        let s3_config = S3Config::from_type_config(config)?;
        S3StorageInner::load_client(&s3_config).await?;
        S3StorageInner::build_cache(&s3_config, &self.storage_config).await?;
        info!(bucket = %s3_config.bucket_name, "Successfully connected to S3 bucket");
        Ok(())
    }
    #[instrument(
        name = "Storage::file_exists",
        fields(storage_type = "s3", repository = %repository, path = %location)
    )]
    async fn file_exists(
        &self,
        repository: uuid::Uuid,
        location: &StoragePath,
    ) -> Result<bool, S3StorageError> {
        let path = self.s3_path(&repository, location);
        self.does_path_exist(&path).await
    }

    #[instrument(
        name = "Storage::delete_repository",
        fields(storage_type = "s3", repository = %repository),
        skip(self)
    )]
    async fn delete_repository(&self, repository: uuid::Uuid) -> Result<(), S3StorageError> {
        let prefix = format!("{repository}/");
        let deadline = Instant::now() + S3_CONTROL_TIMEOUT;
        let mut continuation: Option<String> = None;

        loop {
            let mut request = self
                .aws_client()
                .list_objects_v2()
                .bucket(self.bucket())
                .prefix(&prefix)
                .max_keys(1000);

            if let Some(token) = &continuation {
                request = request.continuation_token(token);
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(S3StorageError::aws_message(
                    "S3 repository deletion exceeded its control deadline",
                ));
            }
            let response = with_timeout(remaining, request.send()).await?;

            let paths: Vec<StoragePath> = response
                .contents()
                .iter()
                .filter_map(|obj| obj.key())
                .filter_map(|key| key.strip_prefix(&prefix))
                .filter(|relative| !relative.is_empty())
                .map(StoragePath::from)
                .collect();

            if !paths.is_empty() {
                self.delete_files_batch(repository, &paths).await?;
            }

            if response.is_truncated().unwrap_or(false) {
                continuation = response
                    .next_continuation_token()
                    .map(|token| token.to_string());
                if continuation.is_some() {
                    continue;
                }
            }

            break;
        }

        self.invalidate_manifest_cache(repository);
        Ok(())
    }

    #[instrument(
        name = "Storage::stream_directory",
        fields(storage_type = "s3", repository = %repository, path = %location),
        skip(self)
    )]
    async fn stream_directory(
        &self,
        repository: Uuid,
        location: &StoragePath,
    ) -> Result<Option<Self::DirectoryStream>, Self::Error> {
        // Determine whether the path represents a directory, even when S3 lacks a placeholder
        // object. We need real metadata to avoid treating directories as files when the caller
        // doesn't include a trailing slash (common for Docker paths like `v2`).
        let Some(meta) = self.get_file_information(repository, location).await? else {
            return Ok(None);
        };

        if meta.is_file() && !location.is_directory() {
            let dir_meta = StorageFileMeta::<DirectoryFileType> {
                name: location.to_string(),
                file_type: DirectoryFileType { file_count: 1 },
                modified: meta.modified,
                created: meta.created,
            };
            return Ok(Some(VecDirectoryListStream::new(vec![meta], dir_meta)));
        }

        let mut prefix = self.s3_path(&repository, location);
        if !prefix.ends_with('/') {
            prefix.push('/');
        }

        let base_prefix = S3StorageInner::strip_repository_prefix(&repository, &prefix);
        let base_prefix = if base_prefix.ends_with('/') {
            base_prefix.to_string()
        } else {
            format!("{base_prefix}/")
        };

        let (objects, prefixes) = self.list_directory_entries(&prefix).await?;
        let observed_at = Local::now().fixed_offset();

        // Convert objects to StorageFileMeta entries using names relative to the requested
        // directory (not repository root) so callers can safely append child segments.
        let mut entries: Vec<StorageFileMeta<FileType>> = Vec::new();

        for obj in objects {
            let key = obj.key;
            if key == prefix {
                // Directory placeholder object
                continue;
            }
            if S3StorageInner::is_hidden_file(&key) {
                continue;
            }
            let full_name = S3StorageInner::strip_repository_prefix(&repository, &key);
            let Some(relative_name) = full_name.strip_prefix(&base_prefix) else {
                continue;
            };
            if relative_name.is_empty() {
                continue;
            }

            let meta = StorageFileMeta::<FileType> {
                name: relative_name.to_string(),
                file_type: FileType::File(FileFileType {
                    file_size: obj.size,
                    mime_type: None,
                    file_hash: FileHashes::default(),
                }),
                modified: obj.last_modified.unwrap_or(observed_at),
                created: obj.last_modified.unwrap_or(observed_at),
            };
            entries.push(meta);
        }

        for prefix_entry in prefixes {
            let full_name = S3StorageInner::strip_repository_prefix(&repository, &prefix_entry);
            let Some(relative_name) = full_name.strip_prefix(&base_prefix) else {
                continue;
            };
            let cleaned = relative_name.trim_end_matches('/');
            if cleaned.is_empty() {
                continue;
            }

            if S3StorageInner::is_hidden_file(cleaned) {
                continue;
            }

            let meta = StorageFileMeta::<FileType> {
                name: cleaned.to_string(),
                file_type: FileType::Directory(DirectoryFileType { file_count: 0 }),
                modified: observed_at,
                created: observed_at,
            };
            entries.push(meta);
        }

        // Sort for stable browse results (lexicographic)
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        let dir_meta = StorageFileMeta::<DirectoryFileType> {
            name: location.to_string(),
            file_type: DirectoryFileType {
                file_count: entries.len() as u64,
            },
            modified: meta.modified,
            created: meta.created,
        };

        Ok(Some(VecDirectoryListStream::new(entries, dir_meta)))
    }
}

impl S3Storage {
    /// List all objects for a repository under an optional prefix, returning repository-relative
    /// keys. Uses S3's paginator to minimize the number of API calls while avoiding per-directory
    /// traversal.
    #[instrument(
        name = "S3Storage::list_repository_objects",
        fields(storage_type = "s3", ?repository, prefix = prefix.unwrap_or("") ),
        skip(self)
    )]
    pub async fn list_repository_objects(
        &self,
        repository: Uuid,
        prefix: Option<&str>,
    ) -> Result<Vec<S3ListedObject>, S3StorageError> {
        let mut s3_prefix = repository.to_string();
        s3_prefix.push('/');
        if let Some(prefix) = prefix.filter(|p| !p.is_empty()) {
            s3_prefix.push_str(prefix);
        }

        let mut paginator = self
            .aws_client()
            .list_objects_v2()
            .bucket(self.bucket())
            .prefix(s3_prefix)
            .max_keys(1000)
            .into_paginator()
            .send();
        let deadline = Instant::now() + S3_CONTROL_TIMEOUT;

        let mut objects = Vec::new();

        while let Some(page) = next_control_page(deadline, paginator.next()).await? {
            for obj in page.contents() {
                let Some(key) = obj.key() else { continue };
                if S3StorageInner::is_hidden_file(key) {
                    continue;
                }

                let repo_relative = S3StorageInner::strip_repository_prefix(&repository, key);

                // Skip the directory placeholder object (equal to the prefix)
                if repo_relative.is_empty() {
                    continue;
                }

                let size = obj.size().unwrap_or(0i64).max(0) as u64;
                let last_modified = s3_last_modified(obj.last_modified());

                objects.push(S3ListedObject {
                    key: repo_relative.to_string(),
                    size,
                    last_modified,
                });
            }
        }

        Ok(objects)
    }

    /// List only Docker manifest objects for a repository without traversing blobs.
    /// This walks prefixes breadth-first and descends until it reaches `manifests/` directories,
    /// skipping `blobs`, `uploads`, `_uploads` to avoid huge listings.
    #[instrument(
        name = "S3Storage::list_docker_manifests",
        fields(storage_type = "s3", ?repository),
        skip(self)
    )]
    async fn load_docker_manifests(
        &self,
        repository: Uuid,
    ) -> Result<Vec<S3ListedObject>, S3StorageError> {
        let mut manifests = Vec::new();
        let deadline = Instant::now() + S3_CONTROL_TIMEOUT;

        let mut queue = VecDeque::new();
        queue.push_back(format!("{}/v2/", repository));

        while let Some(prefix) = queue.pop_front() {
            let mut paginator = self
                .aws_client()
                .list_objects_v2()
                .bucket(self.bucket())
                .prefix(prefix.clone())
                .delimiter("/")
                .max_keys(1000)
                .into_paginator()
                .send();

            while let Some(page) = next_control_page(deadline, paginator.next()).await? {
                for p in page
                    .common_prefixes()
                    .iter()
                    .filter_map(CommonPrefix::prefix)
                {
                    if p.ends_with("blobs/") || p.ends_with("uploads/") || p.ends_with("_uploads/")
                    {
                        continue;
                    }

                    if p.ends_with("manifests/") {
                        // List only manifest objects under this prefix (no delimiter to get files).
                        let mut manifest_pages = self
                            .aws_client()
                            .list_objects_v2()
                            .bucket(self.bucket())
                            .prefix(p)
                            .max_keys(1000)
                            .into_paginator()
                            .send();

                        while let Some(mpage) =
                            next_control_page(deadline, manifest_pages.next()).await?
                        {
                            for obj in mpage.contents() {
                                let Some(key) = obj.key() else { continue };
                                if S3StorageInner::is_hidden_file(key) {
                                    continue;
                                }
                                // Strip repository prefix
                                let repo_relative =
                                    S3StorageInner::strip_repository_prefix(&repository, key);
                                if repo_relative.is_empty() {
                                    continue;
                                }
                                let size = obj.size().unwrap_or(0i64).max(0) as u64;
                                manifests.push(S3ListedObject {
                                    key: repo_relative.to_string(),
                                    size,
                                    last_modified: s3_last_modified(obj.last_modified()),
                                });
                            }
                        }
                    } else {
                        queue.push_back(p.to_string());
                    }
                }
            }
        }

        manifests.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(manifests)
    }

    /// List Docker manifests using a short-lived per-repository index.
    pub async fn list_docker_manifests(
        &self,
        repository: Uuid,
    ) -> Result<Vec<S3ListedObject>, S3StorageError> {
        let cached = self.manifest_cache.lock().get(repository, Instant::now());
        if let Some(cached) = cached {
            return Ok(cached);
        }
        let _load_lock = self.manifest_load_lock.lock().await;
        let generation = {
            let mut cache = self.manifest_cache.lock();
            if let Some(cached) = cache.get(repository, Instant::now()) {
                return Ok(cached);
            }
            cache.generation()
        };
        let manifests = self.load_docker_manifests(repository).await?;
        self.manifest_cache.lock().insert_if_generation(
            repository,
            manifests.clone(),
            Instant::now(),
            generation,
        );
        Ok(manifests)
    }

    /// Paginate Docker manifest objects without loading the entire repository into memory.
    ///
    /// Returns the requested page of manifest objects (ordered lexicographically by key)
    /// and the total number of manifest objects for the repository.
    #[instrument(
        name = "S3Storage::list_docker_manifests_paginated",
        fields(storage_type = "s3", ?repository, start, limit),
        skip(self)
    )]
    pub async fn list_docker_manifests_paginated(
        &self,
        repository: Uuid,
        start: usize,
        limit: usize,
    ) -> Result<(Vec<S3ListedObject>, usize), S3StorageError> {
        let manifests = self.list_docker_manifests(repository).await?;
        let total = manifests.len();
        let items = manifests.into_iter().skip(start).take(limit).collect();
        Ok((items, total))
    }

    async fn collect_directory(
        &self,
        repository: Uuid,
        location: &StoragePath,
        modified: Option<ChronoDateTime<FixedOffset>>,
    ) -> Result<Option<StorageFile>, S3StorageError> {
        let Some(stream) = self.stream_directory(repository, location).await? else {
            return Ok(None);
        };

        let file_count = stream.number_of_files();
        let files = collect_directory_stream(stream)
            .await
            .map_err(|err| S3StorageError::aws_message(err.to_string()))?;
        let observed_at = modified.unwrap_or_else(|| Local::now().fixed_offset());

        let meta = StorageFileMeta::<DirectoryFileType> {
            name: location.to_string(),
            file_type: DirectoryFileType {
                file_count: file_count.max(files.len() as u64),
            },
            modified: observed_at,
            created: observed_at,
        };

        Ok(Some(StorageFile::Directory { meta, files }))
    }

    /// Delete multiple files in batch using S3's delete_objects API.
    /// This is much more efficient than calling delete_file repeatedly.
    /// Can delete up to 1000 objects per API call.
    ///
    /// Returns the number of files actually deleted.
    #[instrument(
        name = "S3Storage::delete_files_batch",
        fields(storage_type = "s3", repository = %repository, count = paths.len()),
        skip(self)
    )]
    pub async fn delete_files_batch(
        &self,
        repository: Uuid,
        paths: &[StoragePath],
    ) -> Result<usize, S3StorageError> {
        if paths.is_empty() {
            return Ok(0);
        }

        use aws_sdk_s3::types::ObjectIdentifier;

        let mut deleted_count = 0;

        // S3 allows max 1000 objects per delete_objects call
        for chunk in paths.chunks(1000) {
            let mut object_ids = Vec::with_capacity(chunk.len());
            let mut keys_for_log = Vec::with_capacity(chunk.len());
            for path in chunk {
                let key = self.s3_path(&repository, path);
                keys_for_log.push(key.clone());
                let obj_id = ObjectIdentifier::builder()
                    .key(key)
                    .build()
                    .map_err(|err| S3StorageError::aws_message(err.to_string()))?;
                object_ids.push(obj_id);
            }

            if object_ids.is_empty() {
                continue;
            }

            let response = with_timeout(
                S3_CONTROL_TIMEOUT,
                self.aws_client()
                    .delete_objects()
                    .bucket(self.bucket())
                    .delete(
                        aws_sdk_s3::types::Delete::builder()
                            .set_objects(Some(object_ids))
                            .quiet(true) // Don't return deleted objects in response
                            .build()
                            .map_err(|err| S3StorageError::aws_message(err.to_string()))?,
                    )
                    .send(),
            )
            .await?;

            // Count successful deletions (errors() returns objects that failed)
            let failed = response.errors();
            for path in chunk {
                self.cache_remove(&repository, path).await?;
            }
            self.invalidate_manifest_cache(repository);
            if !failed.is_empty() {
                let (code, message, key) = failed
                    .first()
                    .map(|first| {
                        (
                            first.code().unwrap_or("unknown"),
                            first.message().unwrap_or("unknown"),
                            first.key().unwrap_or("unknown"),
                        )
                    })
                    .unwrap_or(("unknown", "unknown", "unknown"));
                warn!(
                    repository = %repository,
                    failed = failed.len(),
                    total = chunk.len(),
                    first_key = key,
                    code,
                    message,
                    keys_sample = ?keys_for_log.get(0..5).map(|v| v.to_vec()),
                    "S3 delete_objects reported errors"
                );
                return Err(S3StorageError::aws_message(format!(
                    "delete_objects failed for key {key}: {code} - {message}"
                )));
            }

            deleted_count += chunk.len();
        }

        debug!(
            repository = %repository,
            deleted = deleted_count,
            total = paths.len(),
            "Batch deleted objects from S3"
        );

        Ok(deleted_count)
    }

    /// Calculate total object size for a repository using paginated ListObjectsV2 calls.
    /// Skips internal Pkgly metadata objects.
    pub async fn repository_size_bytes(&self, repository: Uuid) -> Result<u64, S3StorageError> {
        let prefix = format!("{repository}/");
        let deadline = Instant::now() + S3_CONTROL_TIMEOUT;
        let mut continuation: Option<String> = None;
        let mut total: u64 = 0;

        loop {
            let mut request = self
                .aws_client()
                .list_objects_v2()
                .bucket(self.bucket())
                .prefix(prefix.clone());

            if let Some(token) = &continuation {
                request = request.continuation_token(token);
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(S3StorageError::aws_message(
                    "S3 repository size calculation exceeded its control deadline",
                ));
            }
            let response = with_timeout(remaining, request.send()).await?;

            for obj in response.contents() {
                if let Some(key) = obj.key() {
                    if S3StorageInner::is_hidden_file(key) {
                        continue;
                    }
                    let size = obj.size().unwrap_or_default().max(0) as u64;
                    total = total.saturating_add(size);
                }
            }

            if response.is_truncated().unwrap_or(false) {
                continuation = response
                    .next_continuation_token()
                    .map(|token| token.to_string());
                if continuation.is_some() {
                    continue;
                }
            }

            break;
        }

        Ok(total)
    }
}
#[derive(Debug, Default)]
pub struct S3StorageFactory;
impl StaticStorageFactory for S3StorageFactory {
    type StorageType = S3Storage;
    type ConfigType = S3Config;
    type Error = S3StorageError;

    fn storage_type_name() -> &'static str {
        "s3"
    }

    async fn test_storage_config(config: StorageTypeConfig) -> Result<(), S3StorageError> {
        let s3_config = S3Config::from_type_config(config)?;
        S3StorageInner::load_client(&s3_config).await?;
        info!(bucket = %s3_config.bucket_name, "Successfully connected to S3 bucket");
        Ok(())
    }

    async fn create_storage(
        inner: StorageConfigInner,
        type_config: Self::ConfigType,
    ) -> Result<Self::StorageType, S3StorageError> {
        let client = S3StorageInner::load_client(&type_config).await?;
        let cache = S3StorageInner::build_cache(&type_config, &inner).await?;
        let inner = S3StorageInner {
            config: type_config,
            storage_config: inner,
            client,
            cache,
            manifest_cache: ParkingMutex::new(ManifestCache::new()),
            manifest_load_lock: Mutex::new(()),
        };
        let storage = S3Storage::from(inner);
        Ok(storage)
    }
}
impl StorageFactory for S3StorageFactory {
    fn storage_name(&self) -> &'static str {
        "s3"
    }

    fn test_storage_config(
        &self,
        config: StorageTypeConfig,
    ) -> BoxFuture<'static, Result<(), StorageError>> {
        Box::pin(async move {
            let s3_config = S3Config::from_type_config(config)?;

            S3StorageInner::load_client(&s3_config).await?;
            info!(bucket = %s3_config.bucket_name, "Successfully connected to S3 bucket");

            Ok(())
        })
    }

    fn create_storage(
        &self,
        config: StorageConfig,
    ) -> BoxFuture<'static, Result<DynStorage, StorageError>> {
        Box::pin(async move {
            let s3_config = S3Config::from_type_config(config.type_config)?;
            let storage_config = config.storage_config;
            let client = S3StorageInner::load_client(&s3_config).await?;
            let cache = S3StorageInner::build_cache(&s3_config, &storage_config).await?;
            let inner = S3StorageInner {
                config: s3_config,
                storage_config,
                client,
                cache,
                manifest_cache: ParkingMutex::new(ManifestCache::new()),
                manifest_load_lock: Mutex::new(()),
            };
            let storage = S3Storage::from(inner);
            Ok(DynStorage::S3(storage))
        })
    }
}
#[cfg(test)]
mod tests;
