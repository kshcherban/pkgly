#![allow(dead_code)]
use std::{
    borrow::Cow, collections::VecDeque, env, io::ErrorKind, num::NonZeroUsize, ops::Deref,
    path::PathBuf, pin::Pin, str::FromStr, sync::Arc,
};

use aws_config::BehaviorVersion;
use aws_config::sts::AssumeRoleProvider;
use aws_credential_types::{Credentials as AwsCredentials, provider::SharedCredentialsProvider};
use aws_sdk_s3::{
    Client as AwsS3Client,
    types::{CommonPrefix, Tag},
};
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_types::byte_stream::ByteStream;
use aws_types::{SdkConfig, region::Region};
use bytes::Bytes;
use chrono::{FixedOffset, Local};
use futures::future::BoxFuture;
use hex::encode;
use lru::LruCache;
use mime::Mime;
use nr_core::storage::{FileHashes, FileTypeCheck, SerdeMime, StoragePath};
use regions::{CustomRegion, S3StorageRegion};
use sha2::{Digest, Sha256};
use sysinfo::System;
use tokio::{
    fs,
    io::BufReader,
    sync::Mutex,
    task,
    time::{Duration, Instant},
};
use url::Url;

pub mod regions;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument, warn};
use utoipa::ToSchema;
pub mod tags;
use uuid::Uuid;
#[derive(Debug, thiserror::Error)]
pub enum S3StorageError {
    #[error("No Region Provided")]
    NoRegionSpecified,
    #[error("AWS SDK error: {0}")]
    AwsSdkError(String),
    #[error("Bucket Does Not Exist {0}")]
    BucketDoesNotExist(String),
    #[error("IO Error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("Blocking task join error: {0}")]
    BlockingJoin(#[from] tokio::task::JoinError),
    #[error(transparent)]
    InvalidConfigType(#[from] InvalidConfigType),

    #[error("Missing Tag: {0}")]
    MissingTag(Cow<'static, str>),

    #[error(transparent)]
    PathCollision(#[from] PathCollisionError),
}
impl S3StorageError {
    pub fn static_missing_tag(tag: &'static str) -> Self {
        S3StorageError::MissingTag(tag.into())
    }
    pub fn from_sdk_error(err: impl std::fmt::Display) -> Self {
        S3StorageError::AwsSdkError(err.to_string())
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

struct MemorySnapshot {
    total_bytes: u64,
    available_bytes: u64,
}

impl MemorySnapshot {
    fn capture() -> Option<Self> {
        let mut system = System::new();
        system.refresh_memory();
        let total = system.total_memory();
        if total == 0 {
            return None;
        }
        let available = system.available_memory();
        Some(Self {
            total_bytes: total.saturating_mul(1024),
            available_bytes: available.saturating_mul(1024),
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
    pub region: Option<S3StorageRegion>,
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
        if let Some(region) = &self.region {
            return Ok((*region).into());
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
#[derive(Debug, Clone)]
pub struct S3MetaTags {
    pub name: String,
    pub mime_type: Option<Mime>,
    pub is_directory: bool,
}

#[derive(Debug)]
pub(super) struct S3DiskCache {
    dir: PathBuf,
    max_bytes: u64,
    state: Mutex<CacheState>,
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
    content_type: Option<String>,
}

#[derive(Debug, Clone)]
struct CachedObject {
    bytes: Bytes,
    content_type: Option<String>,
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
            return Err(S3StorageError::AwsSdkError(
                "cache max_bytes must be greater than zero".into(),
            ));
        }
        let dir = config
            .path
            .clone()
            .unwrap_or_else(|| default_cache_dir(storage_name));
        fs::create_dir_all(&dir).await?;
        let capacity = NonZeroUsize::new(config.max_entries.max(1)).unwrap_or(NonZeroUsize::MIN);
        let state = CacheState {
            entries: LruCache::new(capacity),
            current_bytes: 0,
            failed_deletions: VecDeque::new(),
        };
        Ok(Self {
            dir,
            max_bytes: config.max_bytes,
            state: Mutex::new(state),
        })
    }

    fn hashed_filename(key: &str) -> PathBuf {
        let digest = Sha256::digest(key.as_bytes());
        let hex = encode(digest);
        let (prefix, rest) = hex.split_at(2);
        PathBuf::from(prefix).join(rest)
    }

    async fn get(&self, key: &str) -> Result<Option<CachedObject>, S3StorageError> {
        self.retry_failed_deletions().await;
        let (relative_path, content_type) = {
            let mut state = self.state.lock().await;
            match state.entries.get(key) {
                Some(entry) => (entry.relative_path.clone(), entry.content_type.clone()),
                None => return Ok(None),
            }
        };
        let path = self.dir.join(relative_path);
        match fs::read(&path).await {
            Ok(data) => Ok(Some(CachedObject {
                bytes: Bytes::from(data),
                content_type,
            })),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    async fn put(
        &self,
        key: &str,
        data: Bytes,
        content_type: Option<&str>,
    ) -> Result<(), S3StorageError> {
        self.retry_failed_deletions().await;
        let relative = Self::hashed_filename(key);
        let path = self.dir.join(&relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&path, data.as_ref()).await?;
        let mut removed = Vec::new();
        {
            let mut state = self.state.lock().await;
            if let Some(old) = state.entries.pop(key) {
                state.current_bytes = state.current_bytes.saturating_sub(old.size);
                removed.push(old.relative_path);
            }
            state.entries.put(
                key.to_string(),
                CacheEntry {
                    relative_path: relative,
                    size: data.len() as u64,
                    content_type: content_type.map(|c| c.to_string()),
                },
            );
            state.current_bytes = state.current_bytes.saturating_add(data.len() as u64);
            while state.current_bytes > self.max_bytes {
                if let Some((_, evicted)) = state.entries.pop_lru() {
                    state.current_bytes = state.current_bytes.saturating_sub(evicted.size);
                    removed.push(evicted.relative_path);
                } else {
                    break;
                }
            }
        }
        for rel in removed {
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

    async fn delete_relative_path(&self, relative: PathBuf) {
        let path = self.dir.join(&relative);
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
#[derive(Debug)]
pub struct S3StorageInner {
    pub config: S3Config,
    pub storage_config: StorageConfigInner,
    pub client: AwsS3Client,
    cache: Option<Arc<S3DiskCache>>,
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

        if let Some(endpoint) = config.custom_endpoint() {
            builder = builder.endpoint_url(endpoint.to_string());
        }

        if let Some(role) = config.credentials.role_to_assume() {
            let assume_provider = build_assume_role_provider(role, &base_config).await?;
            builder = builder.credentials_provider(SharedCredentialsProvider::new(assume_provider));
        } else if let Some(provider) = static_provider {
            builder = builder.credentials_provider(provider);
        }

        let client = AwsS3Client::from_conf(builder.build());
        match client
            .head_bucket()
            .bucket(&config.bucket_name)
            .send()
            .await
        {
            Ok(_) => Ok(client),
            Err(SdkError::ServiceError(err)) if err.err().is_not_found() => Err(
                S3StorageError::BucketDoesNotExist(config.bucket_name.clone()),
            ),
            Err(err) => Err(S3StorageError::from_sdk_error(err)),
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
    ) -> Result<(), S3StorageError> {
        if !self.should_cache(location) {
            return Ok(());
        }
        if let Some(cache) = &self.cache {
            let key = self.cache_key(repository, location);
            let data_len = data.len();
            cache.put(&key, data, content_type.as_deref()).await?;
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
            let exists_as_object = self.does_path_exist(&path).await?;

            if exists_as_object && !is_last {
                // A parent segment is a concrete object, so we cannot place a child under it.
                return Err(PathCollisionError {
                    path: location.clone(),
                    conflicts_with: conflicting_path,
                }
                .into());
            }
            // If this is the last segment, overwriting an existing object is allowed.
        }

        Ok(path)
    }
    #[instrument]
    async fn does_path_exist(&self, path: &str) -> Result<bool, S3StorageError> {
        let result = self
            .aws_client()
            .head_object()
            .bucket(self.bucket())
            .key(path)
            .send()
            .await;

        match result {
            Ok(_) => Ok(true),
            Err(SdkError::ServiceError(err)) if err.err().is_not_found() => Ok(false),
            Err(err) => Err(S3StorageError::from_sdk_error(err)),
        }
    }
    #[instrument]
    fn is_directory_from_result(
        &self,
        result: &aws_sdk_s3::operation::list_objects_v2::ListObjectsV2Output,
        path: &str,
    ) -> (bool, Option<String>) {
        let contents = result.contents();
        let prefixes = result.common_prefixes();
        let is_contents_empty = contents.is_empty();
        let has_prefixes = !prefixes.is_empty();

        if is_contents_empty && !has_prefixes {
            return (true, None);
        }
        if path.ends_with('/') && !is_contents_empty {
            return (true, None);
        }

        let path_with_slash = format!("{}/", path);
        if let Some(match_prefix) = prefixes
            .iter()
            .filter_map(CommonPrefix::prefix)
            .find(|prefix| *prefix == path_with_slash)
        {
            return (true, Some(match_prefix.to_string()));
        }

        (false, None)
    }

    #[instrument]
    async fn is_directory(&self, path: &str) -> Result<bool, S3StorageError> {
        let list = self
            .aws_client()
            .list_objects_v2()
            .bucket(self.bucket())
            .prefix(path.to_owned())
            .delimiter("/")
            .send()
            .await
            .map_err(S3StorageError::from_sdk_error)?;

        Ok(self.is_directory_from_result(&list, path).0)
    }

    async fn get_directory_meta(
        &self,
        path: &str,
    ) -> Result<Option<StorageFileMeta<FileType>>, S3StorageError> {
        let file_file = FileType::Directory(DirectoryFileType { file_count: 0 });

        let name = path.split_once('/').map(|(_, rest)| rest).unwrap_or(path);
        let meta = StorageFileMeta {
            name: name.to_owned(),
            file_type: file_file,
            modified: Local::now().fixed_offset(),
            created: Local::now().fixed_offset(),
        };

        Ok(Some(meta))
    }
    #[instrument]
    async fn get_object_tagging(&self, path: &str) -> Result<Option<Vec<Tag>>, S3StorageError> {
        let response = self
            .aws_client()
            .get_object_tagging()
            .bucket(self.bucket())
            .key(path)
            .send()
            .await;

        match response {
            Ok(output) => Ok(Some(output.tag_set().to_vec())),
            Err(SdkError::ServiceError(err))
                if err
                    .err()
                    .meta()
                    .code()
                    .is_some_and(|code| code == "NoSuchKey") =>
            {
                Ok(None)
            }
            Err(err) => Err(S3StorageError::from_sdk_error(err)),
        }
    }

    async fn get_meta_tags(&self, path: &str) -> Result<Option<S3MetaTags>, S3StorageError> {
        let Some(tags) = self.get_object_tagging(path).await? else {
            return Ok(None);
        };

        let name = tags
            .iter()
            .find(|tag| tag.key() == tags::NAME)
            .map(|tag| tag.value().to_string())
            .ok_or_else(|| S3StorageError::static_missing_tag(tags::NAME))?;

        let mime_type = tags
            .iter()
            .find(|tag| tag.key() == tags::MIME_TYPE)
            .map(|tag| Mime::from_str(tag.value()))
            .transpose();
        let mime_type = match mime_type {
            Ok(ok) => ok,
            Err(e) => {
                error!(?e, ?path, "Failed to parse mime type");
                None
            }
        };

        Ok(Some(S3MetaTags {
            name,
            mime_type,
            is_directory: false,
        }))
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

fn bytes_to_stream(bytes: FileContentBytes) -> (ByteStream, usize) {
    match bytes {
        FileContentBytes::Content(content) => {
            let len = content.len();
            (ByteStream::from(content), len)
        }
        FileContentBytes::Bytes(bytes) => {
            let len = bytes.len();
            (ByteStream::from(bytes.to_vec()), len)
        }
    }
}

async fn file_into_bytes(file: FileContent) -> Result<FileContentBytes, S3StorageError> {
    let bytes = task::spawn_blocking(move || FileContentBytes::try_from(file)).await??;
    Ok(bytes)
}

async fn collect_body(stream: ByteStream) -> Result<Bytes, S3StorageError> {
    let aggregated = stream
        .collect()
        .await
        .map_err(|err| S3StorageError::AwsSdkError(err.to_string()))?;
    Ok(aggregated.into_bytes())
}

const DEFAULT_MIN_BUFFERED_OBJECT_BYTES: u64 = 1024 * 1024; // 1 MiB
const DEFAULT_MAX_BUFFERED_OBJECT_BYTES: u64 = 8 * 1024 * 1024; // 8 MiB
const DEFAULT_MEMORY_PRESSURE_THRESHOLD: f64 = 0.75;
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
    pub last_modified: Option<chrono::DateTime<FixedOffset>>,
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
        let cache_buffer = file_as_bytes.clone_into_bytes();
        let (body, size) = bytes_to_stream(file_as_bytes);
        self.aws_client()
            .put_object()
            .bucket(self.bucket())
            .key(&path)
            .body(body)
            .content_type(content_type)
            .send()
            .await
            .map_err(S3StorageError::from_sdk_error)?;
        debug!(path = %path, "File saved to S3");
        self.cache_put(
            &repository,
            location,
            cache_buffer,
            Some(content_type.to_string()),
        )
        .await?;
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

        let mut combined_buffer = if self.does_path_exist(&path).await? {
            let response = self
                .aws_client()
                .get_object()
                .bucket(self.bucket())
                .key(&path)
                .send()
                .await
                .map_err(S3StorageError::from_sdk_error)?;
            collect_body(response.body).await?.to_vec()
        } else {
            Vec::new()
        };

        let appended = file_into_bytes(file).await?;
        combined_buffer.extend_from_slice(appended.as_ref());

        let combined_bytes = Bytes::from(combined_buffer);

        let content_type = if location.is_directory() {
            "application/x-directory"
        } else {
            "application/octet-stream"
        };
        let size = combined_bytes.len();
        self.aws_client()
            .put_object()
            .bucket(self.bucket())
            .key(&path)
            .content_type(content_type)
            .body(ByteStream::from(combined_bytes.clone()))
            .send()
            .await
            .map_err(S3StorageError::from_sdk_error)?;
        self.cache_put(
            &repository,
            location,
            combined_bytes,
            Some(content_type.to_string()),
        )
        .await?;
        Ok(size)
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
                .max_keys(1)
                .send()
                .await
                .map_err(S3StorageError::from_sdk_error)?;
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

        self.aws_client()
            .put_object()
            .bucket(self.bucket())
            .key(meta_path)
            .content_type("application/json")
            .body(body)
            .send()
            .await
            .map_err(S3StorageError::from_sdk_error)?;

        // Repository meta is small; we intentionally do not cache it to avoid polluting the blob cache.
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

        let response = match self
            .aws_client()
            .get_object()
            .bucket(self.bucket())
            .key(&meta_path)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(SdkError::ServiceError(err)) if err.err().is_no_such_key() => return Ok(None),
            Err(err) => return Err(S3StorageError::from_sdk_error(err)),
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
        self.aws_client()
            .delete_object()
            .bucket(self.bucket())
            .key(&path)
            .send()
            .await
            .map_err(S3StorageError::from_sdk_error)?;
        self.cache_remove(&repository, location).await?;
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

        // Check if source exists
        if !self.does_path_exist(&from_path).await? {
            return Ok(false);
        }

        // For S3, we need to copy and then delete since there's no native rename
        // Read the object
        let response = self
            .aws_client()
            .get_object()
            .bucket(self.bucket())
            .key(&from_path)
            .send()
            .await
            .map_err(S3StorageError::from_sdk_error)?;
        let bytes = collect_body(response.body).await?;

        // Get content type from original object metadata (if available)
        let content_type = if to.is_directory() {
            "application/x-directory"
        } else {
            "application/octet-stream"
        };

        // Write to new location
        self.aws_client()
            .put_object()
            .bucket(self.bucket())
            .key(&to_path)
            .content_type(content_type)
            .body(ByteStream::from(bytes.clone()))
            .send()
            .await
            .map_err(S3StorageError::from_sdk_error)?;

        // Delete original
        self.aws_client()
            .delete_object()
            .bucket(self.bucket())
            .key(&from_path)
            .send()
            .await
            .map_err(S3StorageError::from_sdk_error)?;

        self.cache_remove(&repository, from).await?;
        self.cache_put(&repository, to, bytes, Some(content_type.to_string()))
            .await?;

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
            return Ok(Some(StorageFileMeta::<FileType> {
                name: location.to_string(),
                file_type: FileType::File(FileFileType {
                    file_size: size,
                    mime_type,
                    file_hash: FileHashes::default(),
                }),
                modified: Local::now().fixed_offset(),
                created: Local::now().fixed_offset(),
            }));
        }

        let path = self.s3_path(&repository, location);
        let head = match self
            .aws_client()
            .head_object()
            .bucket(self.bucket())
            .key(&path)
            .send()
            .await
        {
            Ok(head) => head,
            Err(SdkError::ServiceError(err)) if err.err().is_not_found() => {
                // Maybe this is a directory prefix without a placeholder object.
                let prefix = if path.ends_with('/') {
                    path.clone()
                } else {
                    format!("{}/", path)
                };
                let list = self
                    .aws_client()
                    .list_objects_v2()
                    .bucket(self.bucket())
                    .prefix(prefix.clone())
                    .delimiter("/")
                    .send()
                    .await
                    .map_err(S3StorageError::from_sdk_error)?;

                if list.key_count().unwrap_or(0) == 0 {
                    return Ok(None);
                }

                let mut count: u64 = 0;
                for obj in list.contents() {
                    if let Some(key) = obj.key() {
                        if key == prefix || S3StorageInner::is_hidden_file(key) {
                            continue;
                        }
                        count += 1;
                    }
                }
                for pref in list
                    .common_prefixes()
                    .iter()
                    .filter_map(CommonPrefix::prefix)
                {
                    if S3StorageInner::is_hidden_file(pref) {
                        continue;
                    }
                    count += 1;
                }

                let dir_meta = StorageFileMeta::<FileType> {
                    name: location.to_string(),
                    file_type: FileType::Directory(DirectoryFileType { file_count: count }),
                    modified: Local::now().fixed_offset(),
                    created: Local::now().fixed_offset(),
                };
                return Ok(Some(dir_meta));
            }
            Err(err) => return Err(S3StorageError::from_sdk_error(err)),
        };

        let content_type = head.content_type().map(|ct| ct.to_string());
        if content_type
            .as_deref()
            .is_some_and(|ct| ct == "application/x-directory")
            && let Some(meta) = self.get_directory_meta(&path).await?
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

        let modified = Local::now().fixed_offset();

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
            let meta = StorageFileMeta::<FileFileType> {
                name: location.to_string(),
                file_type: FileFileType {
                    file_size: size,
                    mime_type,
                    file_hash: FileHashes::default(),
                },
                modified: Local::now().fixed_offset(),
                created: Local::now().fixed_offset(),
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
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(SdkError::ServiceError(err)) if err.err().is_no_such_key() => {
                return self.collect_directory(repository, location).await;
            }
            Err(err) => return Err(S3StorageError::from_sdk_error(err)),
        };

        let response_content_type = response.content_type().map(|ct| ct.to_string());
        if response_content_type
            .as_deref()
            .map(|ct| ct == "application/x-directory")
            .unwrap_or(false)
        {
            return self.collect_directory(repository, location).await;
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
            modified: Local::now().fixed_offset(),
            created: Local::now().fixed_offset(),
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

            let response = request
                .send()
                .await
                .map_err(S3StorageError::from_sdk_error)?;

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

        let list = self
            .aws_client()
            .list_objects_v2()
            .bucket(self.bucket())
            .prefix(prefix.clone())
            .delimiter("/")
            .send()
            .await
            .map_err(S3StorageError::from_sdk_error)?;

        // Convert objects to StorageFileMeta entries using names relative to the requested
        // directory (not repository root) so callers can safely append child segments.
        let mut entries: Vec<StorageFileMeta<FileType>> = Vec::new();

        for obj in list.contents() {
            let Some(key) = obj.key() else { continue };
            if key == prefix {
                // Directory placeholder object
                continue;
            }
            if S3StorageInner::is_hidden_file(key) {
                continue;
            }
            let full_name = S3StorageInner::strip_repository_prefix(&repository, key);
            let Some(relative_name) = full_name.strip_prefix(&base_prefix) else {
                continue;
            };
            if relative_name.is_empty() {
                continue;
            }

            let size: u64 = obj.size().unwrap_or(0i64).max(0) as u64;
            let meta = StorageFileMeta::<FileType> {
                name: relative_name.to_string(),
                file_type: FileType::File(FileFileType {
                    file_size: size,
                    mime_type: None,
                    file_hash: FileHashes::default(),
                }),
                modified: Local::now().fixed_offset(),
                created: Local::now().fixed_offset(),
            };
            entries.push(meta);
        }

        for prefix_entry in list
            .common_prefixes()
            .iter()
            .filter_map(CommonPrefix::prefix)
        {
            let full_name = S3StorageInner::strip_repository_prefix(&repository, prefix_entry);
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
                modified: Local::now().fixed_offset(),
                created: Local::now().fixed_offset(),
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

        let mut objects = Vec::new();

        while let Some(page) = paginator.next().await {
            let page = page.map_err(S3StorageError::from_sdk_error)?;

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
                let last_modified = None;

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
    pub async fn list_docker_manifests(
        &self,
        repository: Uuid,
    ) -> Result<Vec<S3ListedObject>, S3StorageError> {
        let mut manifests = Vec::new();

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

            while let Some(page) = paginator.next().await {
                let page = page.map_err(S3StorageError::from_sdk_error)?;

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

                        while let Some(mpage) = manifest_pages.next().await {
                            let mpage = mpage.map_err(S3StorageError::from_sdk_error)?;
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
                                    last_modified: None,
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
        let mut items = Vec::with_capacity(limit);
        let mut total = 0usize;

        // Breadth-first traversal that skips heavy prefixes (`blobs`, uploads) while preserving
        // lexicographic order of manifest keys.
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

            while let Some(page) = paginator.next().await {
                let page = page.map_err(S3StorageError::from_sdk_error)?;

                // Descend into sub-prefixes (directories)
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
                        // List manifest objects directly under this prefix (no delimiter)
                        let mut manifest_pages = self
                            .aws_client()
                            .list_objects_v2()
                            .bucket(self.bucket())
                            .prefix(p)
                            .max_keys(1000)
                            .into_paginator()
                            .send();

                        while let Some(mpage) = manifest_pages.next().await {
                            let mpage = mpage.map_err(S3StorageError::from_sdk_error)?;
                            for obj in mpage.contents() {
                                let Some(key) = obj.key() else { continue };
                                if S3StorageInner::is_hidden_file(key) {
                                    continue;
                                }

                                total += 1;
                                if total <= start {
                                    continue;
                                }
                                if items.len() >= limit {
                                    continue;
                                }

                                let repo_relative =
                                    S3StorageInner::strip_repository_prefix(&repository, key);
                                if repo_relative.is_empty() {
                                    continue;
                                }

                                let size = obj.size().unwrap_or(0i64).max(0) as u64;
                                items.push(S3ListedObject {
                                    key: repo_relative.to_string(),
                                    size,
                                    last_modified: None,
                                });
                            }
                        }
                    } else {
                        queue.push_back(p.to_string());
                    }
                }
            }
        }

        Ok((items, total))
    }

    async fn collect_directory(
        &self,
        repository: Uuid,
        location: &StoragePath,
    ) -> Result<Option<StorageFile>, S3StorageError> {
        let Some(stream) = self.stream_directory(repository, location).await? else {
            return Ok(None);
        };

        let file_count = stream.number_of_files();
        let files = collect_directory_stream(stream)
            .await
            .map_err(|err| S3StorageError::AwsSdkError(err.to_string()))?;

        let meta = StorageFileMeta::<DirectoryFileType> {
            name: location.to_string(),
            file_type: DirectoryFileType {
                file_count: file_count.max(files.len() as u64),
            },
            modified: Local::now().fixed_offset(),
            created: Local::now().fixed_offset(),
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
                    .map_err(|err| S3StorageError::AwsSdkError(err.to_string()))?;
                object_ids.push(obj_id);
            }

            if object_ids.is_empty() {
                continue;
            }

            let response = self
                .aws_client()
                .delete_objects()
                .bucket(self.bucket())
                .delete(
                    aws_sdk_s3::types::Delete::builder()
                        .set_objects(Some(object_ids))
                        .quiet(true) // Don't return deleted objects in response
                        .build()
                        .map_err(|err| S3StorageError::AwsSdkError(err.to_string()))?,
                )
                .send()
                .await
                .map_err(S3StorageError::from_sdk_error)?;

            // Count successful deletions (errors() returns objects that failed)
            let failed = response.errors();
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
                return Err(S3StorageError::AwsSdkError(format!(
                    "delete_objects failed for key {key}: {code} - {message}"
                )));
            }

            deleted_count += chunk.len();

            // Remove from cache
            for path in chunk {
                self.cache_remove(&repository, path).await?;
            }
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

            let response = request
                .send()
                .await
                .map_err(S3StorageError::from_sdk_error)?;

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
            };
            let storage = S3Storage::from(inner);
            Ok(DynStorage::S3(storage))
        })
    }
}
#[cfg(test)]
mod tests;
