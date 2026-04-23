use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, anyhow};
use chrono::{DateTime, Utc};
use http::HeaderName;
use nr_core::database::entities::{
    repository::DBRepositoryWithStorageName,
    user::{UserSafeData, UserType},
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use tokio::{sync::Notify, task::JoinHandle};
use tracing::{info, warn};
use url::Url;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::app::Pkgly;

const DELIVERY_POLL_INTERVAL: Duration = Duration::from_secs(30);
const DELIVERY_CLAIM_TTL: Duration = Duration::from_secs(300);
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_DELIVERY_ATTEMPTS: i32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum WebhookEventType {
    #[serde(rename = "package.published")]
    PackagePublished,
    #[serde(rename = "package.deleted")]
    PackageDeleted,
    #[serde(rename = "package.promoted")]
    PackagePromoted,
}

impl WebhookEventType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PackagePublished => "package.published",
            Self::PackageDeleted => "package.deleted",
            Self::PackagePromoted => "package.promoted",
        }
    }
}

impl std::fmt::Display for WebhookEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for WebhookEventType {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "package.published" => Ok(Self::PackagePublished),
            "package.deleted" => Ok(Self::PackageDeleted),
            "package.promoted" => Ok(Self::PackagePromoted),
            other => Err(anyhow!("Unsupported webhook event type `{other}`")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WebhookDeliveryStatus {
    Pending,
    Processing,
    Delivered,
    Failed,
}

impl WebhookDeliveryStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Delivered => "delivered",
            Self::Failed => "failed",
        }
    }
}

impl std::fmt::Display for WebhookDeliveryStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for WebhookDeliveryStatus {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "processing" => Ok(Self::Processing),
            "delivered" => Ok(Self::Delivered),
            "failed" => Ok(Self::Failed),
            other => Err(anyhow!("Unsupported webhook delivery status `{other}`")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookHeaderInput {
    pub name: String,
    pub value: Option<String>,
    pub configured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpsertWebhookInput {
    pub name: String,
    pub enabled: bool,
    pub target_url: String,
    pub events: Vec<WebhookEventType>,
    pub headers: Vec<WebhookHeaderInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookHeaderSummary {
    pub name: String,
    pub configured: bool,
}

#[derive(Debug, Clone)]
pub struct WebhookSummary {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub target_url: String,
    pub events: Vec<WebhookEventType>,
    pub headers: Vec<WebhookHeaderSummary>,
    pub last_delivery_status: Option<WebhookDeliveryStatus>,
    pub last_delivery_at: Option<DateTime<Utc>>,
    pub last_http_status: Option<i32>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PackageWebhookActor {
    pub user_id: Option<i32>,
    pub username: Option<String>,
}

impl PackageWebhookActor {
    pub fn from_user(user: &UserSafeData) -> Self {
        Self {
            user_id: Some(user.id),
            username: Some(user.username.as_ref().to_string()),
        }
    }

    async fn resolve(self, database: &PgPool) -> anyhow::Result<Self> {
        if self.username.is_some() || self.user_id.is_none() {
            return Ok(self);
        }
        let Some(user_id) = self.user_id else {
            return Ok(self);
        };
        let resolved = UserSafeData::get_by_id(user_id, database)
            .await
            .with_context(|| format!("Failed to load actor details for user {user_id}"))?;
        Ok(Self {
            user_id: Some(user_id),
            username: resolved.map(|user| user.username.as_ref().to_string()),
        })
    }
}

#[derive(Debug, Clone)]
pub struct PackageWebhookSnapshot {
    pub event_type: WebhookEventType,
    pub occurred_at: DateTime<Utc>,
    pub canonical_path: String,
    pub actor: PackageWebhookActor,
    repository: RepositorySnapshot,
    package: PackageSnapshot,
}

#[derive(Debug, Clone)]
struct RepositorySnapshot {
    id: Uuid,
    name: String,
    storage_name: String,
    repository_type: String,
}

#[derive(Debug, Clone)]
struct PackageSnapshot {
    scope: Option<String>,
    package_key: Option<String>,
    package_name: Option<String>,
    project_path: Option<String>,
    version: Option<String>,
    version_path: Option<String>,
}

#[derive(Debug, Clone)]
struct StoredWebhook {
    id: Uuid,
    name: String,
    enabled: bool,
    target_url: String,
    events: Vec<WebhookEventType>,
    headers: BTreeMap<String, String>,
}

#[derive(Debug)]
pub struct WebhookService {
    notify_new_work: Arc<Notify>,
    notify_shutdown: Arc<Notify>,
    handle: JoinHandle<()>,
}

impl WebhookService {
    pub fn start(database: PgPool) -> anyhow::Result<Self> {
        let notify_new_work = Arc::new(Notify::new());
        let notify_shutdown = Arc::new(Notify::new());
        let worker_notify = notify_new_work.clone();
        let shutdown_notify = notify_shutdown.clone();
        let client = Client::builder()
            .timeout(DELIVERY_TIMEOUT)
            .user_agent("Pkgly Webhooks")
            .build()
            .context("Failed to build webhook HTTP client")?;
        let handle = tokio::spawn(async move {
            run_delivery_loop(database, client, worker_notify, shutdown_notify).await;
        });
        Ok(Self {
            notify_new_work,
            notify_shutdown,
            handle,
        })
    }

    pub fn notify_new_work(&self) {
        self.notify_new_work.notify_one();
    }

    pub fn notify_shutdown(&self) {
        self.notify_shutdown.notify_waiters();
    }

    pub fn abort(&self) {
        self.handle.abort();
    }
}

pub async fn list_webhooks(database: &PgPool) -> anyhow::Result<Vec<WebhookSummary>> {
    let rows = sqlx::query(
        r#"
        SELECT
            w.id,
            w.name,
            w.enabled,
            w.target_url,
            w.events,
            w.headers,
            d.status AS last_delivery_status,
            COALESCE(d.delivered_at, d.last_attempt_at, d.created_at) AS last_delivery_at,
            d.last_http_status,
            d.last_error
        FROM webhooks w
        LEFT JOIN LATERAL (
            SELECT status, delivered_at, last_attempt_at, created_at, last_http_status, last_error
            FROM webhook_deliveries
            WHERE webhook_id = w.id
            ORDER BY created_at DESC, id DESC
            LIMIT 1
        ) d ON TRUE
        ORDER BY w.created_at ASC, w.name ASC
        "#,
    )
    .fetch_all(database)
    .await
    .context("Failed to list webhooks")?;

    rows.into_iter().map(webhook_summary_from_row).collect()
}

pub async fn get_webhook(database: &PgPool, id: Uuid) -> anyhow::Result<Option<WebhookSummary>> {
    let row = sqlx::query(
        r#"
        SELECT
            w.id,
            w.name,
            w.enabled,
            w.target_url,
            w.events,
            w.headers,
            d.status AS last_delivery_status,
            COALESCE(d.delivered_at, d.last_attempt_at, d.created_at) AS last_delivery_at,
            d.last_http_status,
            d.last_error
        FROM webhooks w
        LEFT JOIN LATERAL (
            SELECT status, delivered_at, last_attempt_at, created_at, last_http_status, last_error
            FROM webhook_deliveries
            WHERE webhook_id = w.id
            ORDER BY created_at DESC, id DESC
            LIMIT 1
        ) d ON TRUE
        WHERE w.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(database)
    .await
    .with_context(|| format!("Failed to load webhook {id}"))?;

    row.map(webhook_summary_from_row).transpose()
}

pub async fn create_webhook(
    database: &PgPool,
    input: UpsertWebhookInput,
) -> anyhow::Result<WebhookSummary> {
    let validated = validate_webhook_input(None, input)?;
    let id = Uuid::new_v4();

    sqlx::query(
        r#"
        INSERT INTO webhooks (id, name, enabled, target_url, events, headers)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(id)
    .bind(&validated.name)
    .bind(validated.enabled)
    .bind(&validated.target_url)
    .bind(to_json_events(&validated.events))
    .bind(to_json_headers(&validated.headers))
    .execute(database)
    .await
    .context("Failed to create webhook")?;

    get_webhook(database, id)
        .await?
        .ok_or_else(|| anyhow!("Created webhook {id} could not be reloaded"))
}

pub async fn update_webhook(
    database: &PgPool,
    id: Uuid,
    input: UpsertWebhookInput,
) -> anyhow::Result<Option<WebhookSummary>> {
    let Some(current) = load_stored_webhook(database, id).await? else {
        return Ok(None);
    };
    let validated = validate_webhook_input(Some(&current), input)?;

    let updated = sqlx::query(
        r#"
        UPDATE webhooks
        SET name = $2,
            enabled = $3,
            target_url = $4,
            events = $5,
            headers = $6,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(&validated.name)
    .bind(validated.enabled)
    .bind(&validated.target_url)
    .bind(to_json_events(&validated.events))
    .bind(to_json_headers(&validated.headers))
    .execute(database)
    .await
    .with_context(|| format!("Failed to update webhook {id}"))?;

    if updated.rows_affected() == 0 {
        return Ok(None);
    }

    get_webhook(database, id).await
}

pub async fn delete_webhook(database: &PgPool, id: Uuid) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM webhooks WHERE id = $1")
        .bind(id)
        .execute(database)
        .await
        .with_context(|| format!("Failed to delete webhook {id}"))?;
    Ok(result.rows_affected() > 0)
}

pub async fn build_package_event_snapshot(
    site: &Pkgly,
    repository_id: Uuid,
    event_type: WebhookEventType,
    canonical_path: impl Into<String>,
    actor: PackageWebhookActor,
    require_catalog_match: bool,
) -> anyhow::Result<Option<PackageWebhookSnapshot>> {
    let canonical_path = canonical_path.into();
    let Some(repository) = DBRepositoryWithStorageName::get_by_id(repository_id, &site.database)
        .await
        .with_context(|| format!("Failed to load repository {repository_id}"))?
    else {
        return Ok(None);
    };
    let actor = actor.resolve(&site.database).await?;
    let package = resolve_package_snapshot(&site.database, repository_id, &canonical_path).await?;
    if require_catalog_match && package.is_none() {
        return Ok(None);
    }

    let package = package.unwrap_or_else(|| PackageSnapshot {
        scope: None,
        package_key: None,
        package_name: canonical_path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .next_back()
            .map(str::to_string),
        project_path: None,
        version: None,
        version_path: None,
    });

    Ok(Some(PackageWebhookSnapshot {
        event_type,
        occurred_at: Utc::now(),
        canonical_path,
        actor,
        repository: RepositorySnapshot {
            id: repository.id,
            name: repository.name.to_string(),
            storage_name: repository.storage_name.to_string(),
            repository_type: repository.repository_type,
        },
        package,
    }))
}

pub async fn enqueue_snapshot(site: &Pkgly, snapshot: PackageWebhookSnapshot) -> anyhow::Result<usize> {
    let event_filter = json!([snapshot.event_type.as_str()]);
    let rows = sqlx::query(
        r#"
        SELECT id, name, enabled, target_url, events, headers
        FROM webhooks
        WHERE enabled = TRUE AND events @> $1::jsonb
        ORDER BY created_at ASC
        "#,
    )
    .bind(event_filter)
    .fetch_all(&site.database)
    .await
    .context("Failed to load matching webhooks for event enqueue")?;

    if rows.is_empty() {
        return Ok(0);
    }

    let mut enqueued = 0usize;
    for row in rows {
        let webhook = stored_webhook_from_row(&row)?;
        let subscription_key = webhook.id.to_string();
        let payload = build_delivery_payload(&snapshot, &subscription_key);
        sqlx::query(
            r#"
            INSERT INTO webhook_deliveries (
                webhook_id,
                webhook_name,
                event_type,
                subscription_key,
                target_url,
                headers,
                payload,
                status,
                attempts,
                max_attempts,
                next_attempt_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 0, $9, NOW())
            "#,
        )
        .bind(webhook.id)
        .bind(&webhook.name)
        .bind(snapshot.event_type.as_str())
        .bind(&subscription_key)
        .bind(&webhook.target_url)
        .bind(to_json_headers(&webhook.headers))
        .bind(&payload)
        .bind(WebhookDeliveryStatus::Pending.as_str())
        .bind(MAX_DELIVERY_ATTEMPTS)
        .execute(&site.database)
        .await
        .with_context(|| format!("Failed to enqueue webhook delivery for {}", webhook.name))?;
        enqueued += 1;
    }

    site.notify_webhook_worker();
    Ok(enqueued)
}

pub async fn enqueue_package_path_event(
    site: &Pkgly,
    repository_id: Uuid,
    event_type: WebhookEventType,
    canonical_path: impl Into<String>,
    actor: PackageWebhookActor,
    require_catalog_match: bool,
) -> anyhow::Result<usize> {
    let Some(snapshot) = build_package_event_snapshot(
        site,
        repository_id,
        event_type,
        canonical_path,
        actor,
        require_catalog_match,
    )
    .await?
    else {
        return Ok(0);
    };
    enqueue_snapshot(site, snapshot).await
}

fn validate_webhook_input(
    current: Option<&StoredWebhook>,
    input: UpsertWebhookInput,
) -> anyhow::Result<StoredWebhook> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(anyhow!("Webhook name is required"));
    }

    let target_url = input.target_url.trim().to_string();
    let parsed = Url::parse(&target_url).context("Webhook target URL is invalid")?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(anyhow!("Webhook target URL must use http or https"));
    }

    let mut seen_events = HashSet::new();
    let mut events = Vec::new();
    for event in input.events {
        if seen_events.insert(event.as_str()) {
            events.push(event);
        }
    }
    if events.is_empty() {
        return Err(anyhow!("Select at least one webhook event"));
    }

    let existing_headers = current.map(|value| &value.headers);
    let headers = merge_headers(existing_headers, input.headers)?;

    Ok(StoredWebhook {
        id: current.map(|value| value.id).unwrap_or_else(Uuid::new_v4),
        name,
        enabled: input.enabled,
        target_url,
        events,
        headers,
    })
}

fn merge_headers(
    current: Option<&BTreeMap<String, String>>,
    headers: Vec<WebhookHeaderInput>,
) -> anyhow::Result<BTreeMap<String, String>> {
    let mut merged = BTreeMap::new();
    let mut seen = HashSet::new();
    let mut existing_lookup: HashMap<String, (&String, &String)> = HashMap::new();
    if let Some(current) = current {
        for (name, value) in current {
            existing_lookup.insert(name.to_ascii_lowercase(), (name, value));
        }
    }

    for header in headers {
        let trimmed_name = header.name.trim();
        if trimmed_name.is_empty() {
            return Err(anyhow!("Webhook header name is required"));
        }
        HeaderName::from_bytes(trimmed_name.as_bytes())
            .map_err(|_| anyhow!("Invalid webhook header name `{trimmed_name}`"))?;
        let normalized = trimmed_name.to_ascii_lowercase();
        if !seen.insert(normalized.clone()) {
            return Err(anyhow!("Duplicate webhook header `{trimmed_name}`"));
        }

        match header.value {
            Some(value) => {
                let trimmed_value = value.trim().to_string();
                if trimmed_value.is_empty() {
                    return Err(anyhow!(
                        "Webhook header `{trimmed_name}` cannot have an empty value"
                    ));
                }
                merged.insert(trimmed_name.to_string(), trimmed_value);
            }
            None if header.configured => {
                let Some((stored_name, stored_value)) = existing_lookup.get(&normalized) else {
                    return Err(anyhow!(
                        "Webhook header `{trimmed_name}` is marked as configured but no stored value exists"
                    ));
                };
                merged.insert((*stored_name).clone(), (*stored_value).clone());
            }
            None => {
                return Err(anyhow!(
                    "Provide a value for webhook header `{trimmed_name}` or remove it"
                ));
            }
        }
    }

    Ok(merged)
}

fn webhook_summary_from_row(row: sqlx::postgres::PgRow) -> anyhow::Result<WebhookSummary> {
    let headers = parse_header_summaries(row.try_get::<Value, _>("headers")?)?;
    let events = parse_events(row.try_get::<Value, _>("events")?)?;
    let last_delivery_status = row
        .try_get::<Option<String>, _>("last_delivery_status")?
        .map(|value| value.parse())
        .transpose()?;
    let last_delivery_at = row.try_get::<Option<DateTime<Utc>>, _>("last_delivery_at")?;

    Ok(WebhookSummary {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        enabled: row.try_get("enabled")?,
        target_url: row.try_get("target_url")?,
        events,
        headers,
        last_delivery_status,
        last_delivery_at,
        last_http_status: row.try_get("last_http_status")?,
        last_error: row.try_get("last_error")?,
    })
}

async fn load_stored_webhook(database: &PgPool, id: Uuid) -> anyhow::Result<Option<StoredWebhook>> {
    let row = sqlx::query(
        "SELECT id, name, enabled, target_url, events, headers FROM webhooks WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(database)
    .await
    .with_context(|| format!("Failed to load stored webhook {id}"))?;

    row.as_ref().map(stored_webhook_from_row).transpose()
}

fn stored_webhook_from_row(row: &sqlx::postgres::PgRow) -> anyhow::Result<StoredWebhook> {
    Ok(StoredWebhook {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        enabled: row.try_get("enabled")?,
        target_url: row.try_get("target_url")?,
        events: parse_events(row.try_get::<Value, _>("events")?)?,
        headers: parse_headers(row.try_get::<Value, _>("headers")?)?,
    })
}

fn parse_events(value: Value) -> anyhow::Result<Vec<WebhookEventType>> {
    let Value::Array(items) = value else {
        return Err(anyhow!("Webhook events payload is not an array"));
    };
    items.into_iter()
        .map(|item| {
            let Value::String(value) = item else {
                return Err(anyhow!("Webhook event entry is not a string"));
            };
            value.parse()
        })
        .collect()
}

fn parse_headers(value: Value) -> anyhow::Result<BTreeMap<String, String>> {
    let Value::Object(map) = value else {
        return Err(anyhow!("Webhook headers payload is not an object"));
    };
    map.into_iter()
        .map(|(name, value)| match value {
            Value::String(secret) => Ok((name, secret)),
            _ => Err(anyhow!("Webhook header `{name}` must be a string")),
        })
        .collect()
}

fn parse_header_summaries(value: Value) -> anyhow::Result<Vec<WebhookHeaderSummary>> {
    let mut headers = parse_headers(value)?
        .into_keys()
        .map(|name| WebhookHeaderSummary {
            name,
            configured: true,
        })
        .collect::<Vec<_>>();
    headers.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(headers)
}

fn to_json_events(events: &[WebhookEventType]) -> Value {
    Value::Array(
        events
            .iter()
            .map(|event| Value::String(event.as_str().to_string()))
            .collect(),
    )
}

fn to_json_headers(headers: &BTreeMap<String, String>) -> Value {
    let map = headers
        .iter()
        .map(|(name, value)| (name.clone(), Value::String(value.clone())))
        .collect();
    Value::Object(map)
}

async fn resolve_package_snapshot(
    database: &PgPool,
    repository_id: Uuid,
    canonical_path: &str,
) -> anyhow::Result<Option<PackageSnapshot>> {
    let row = sqlx::query(
        r#"
        SELECT
            p.scope,
            p.key AS package_key,
            p.name AS package_name,
            p.path AS project_path,
            v.version,
            v.path AS version_path
        FROM projects p
        LEFT JOIN project_versions v ON v.project_id = p.id
        WHERE p.repository_id = $1
          AND (
            LOWER(COALESCE(v.path, '')) = LOWER($2)
            OR LOWER(p.path) = LOWER($2)
            OR LOWER($2) LIKE LOWER(COALESCE(v.path, '')) || '/%'
            OR LOWER($2) LIKE LOWER(p.path) || '/%'
          )
        ORDER BY
            CASE
                WHEN LOWER(COALESCE(v.path, '')) = LOWER($2) THEN 0
                WHEN LOWER(p.path) = LOWER($2) THEN 1
                WHEN LOWER($2) LIKE LOWER(COALESCE(v.path, '')) || '/%' THEN 2
                ELSE 3
            END,
            LENGTH(COALESCE(v.path, '')) DESC,
            LENGTH(p.path) DESC
        LIMIT 1
        "#,
    )
    .bind(repository_id)
    .bind(canonical_path)
    .fetch_optional(database)
    .await
    .with_context(|| format!("Failed to resolve package snapshot for path `{canonical_path}`"))?;

    let row = if row.is_some() {
        row
    } else if let Some((repository_name, reference)) = parse_manifest_reference_path(canonical_path)
    {
        sqlx::query(
            r#"
            SELECT
                p.scope,
                p.key AS package_key,
                p.name AS package_name,
                p.path AS project_path,
                v.version,
                v.path AS version_path
            FROM projects p
            INNER JOIN project_versions v ON v.project_id = p.id
            WHERE p.repository_id = $1
              AND LOWER(v.version) = LOWER($2)
              AND (
                LOWER(p.key) = LOWER($3)
                OR LOWER(p.name) = LOWER($3)
              )
            ORDER BY LENGTH(COALESCE(v.path, '')) DESC
            LIMIT 1
            "#,
        )
        .bind(repository_id)
        .bind(reference)
        .bind(repository_name)
        .fetch_optional(database)
        .await
        .with_context(|| format!("Failed to resolve package snapshot for manifest path `{canonical_path}`"))?
    } else {
        None
    };

    row.map(|row| {
        Ok(PackageSnapshot {
            scope: row.try_get("scope")?,
            package_key: row.try_get("package_key")?,
            package_name: row.try_get("package_name")?,
            project_path: row.try_get("project_path")?,
            version: row.try_get("version")?,
            version_path: row.try_get("version_path")?,
        })
    })
    .transpose()
}

fn parse_manifest_reference_path(path: &str) -> Option<(String, String)> {
    let trimmed = path.trim_start_matches('/');
    if !trimmed.starts_with("v2/") {
        return None;
    }
    let segments: Vec<&str> = trimmed.split('/').collect();
    let manifest_idx = segments.iter().position(|segment| *segment == "manifests")?;
    if manifest_idx < 2 || manifest_idx + 1 >= segments.len() {
        return None;
    }
    let repository = segments[1..manifest_idx].join("/");
    let reference = segments[manifest_idx + 1].to_string();
    if repository.is_empty() || reference.is_empty() {
        return None;
    }
    Some((repository, reference))
}

fn build_delivery_payload(snapshot: &PackageWebhookSnapshot, subscription_key: &str) -> Value {
    json!({
        "domain": "package",
        "event_type": snapshot.event_type.as_str(),
        "event_id": Uuid::new_v4(),
        "occurred_at": snapshot.occurred_at,
        "subscription_key": subscription_key,
        "source": {
            "application": "pkgly",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "data": {
            "actor": {
                "id": snapshot.actor.user_id,
                "username": snapshot.actor.username,
            },
            "repository": {
                "id": snapshot.repository.id,
                "name": snapshot.repository.name,
                "storage_name": snapshot.repository.storage_name,
                "format": snapshot.repository.repository_type,
            },
            "storage": {
                "name": snapshot.repository.storage_name,
            },
            "package": {
                "scope": snapshot.package.scope,
                "key": snapshot.package.package_key,
                "name": snapshot.package.package_name,
                "version": snapshot.package.version,
                "project_path": snapshot.package.project_path,
                "version_path": snapshot.package.version_path,
                "canonical_path": snapshot.canonical_path,
                "reference": snapshot
                    .package
                    .version
                    .clone()
                    .unwrap_or_else(|| snapshot.canonical_path.clone()),
            },
        }
    })
}

async fn run_delivery_loop(
    database: PgPool,
    client: Client,
    notify_new_work: Arc<Notify>,
    notify_shutdown: Arc<Notify>,
) {
    let mut interval = tokio::time::interval(DELIVERY_POLL_INTERVAL);
    interval.tick().await;

    loop {
        tokio::select! {
            _ = notify_shutdown.notified() => {
                info!("Webhook delivery service shutting down");
                break;
            }
            _ = notify_new_work.notified() => {
                if let Err(err) = process_due_deliveries(&database, &client).await {
                    warn!(error = %err, "Webhook delivery worker wake-up failed");
                }
            }
            _ = interval.tick() => {
                if let Err(err) = process_due_deliveries(&database, &client).await {
                    warn!(error = %err, "Webhook delivery polling tick failed");
                }
            }
        }
    }
}

async fn process_due_deliveries(database: &PgPool, client: &Client) -> anyhow::Result<()> {
    while let Some(delivery) = claim_next_delivery(database).await? {
        let result = deliver_once(client, &delivery).await;
        finalize_delivery_attempt(database, &delivery, result).await?;
    }
    Ok(())
}

#[derive(Debug)]
struct ClaimedDelivery {
    id: i64,
    claim_token: Uuid,
    target_url: String,
    headers: BTreeMap<String, String>,
    payload: Value,
    attempts: i32,
}

async fn claim_next_delivery(database: &PgPool) -> anyhow::Result<Option<ClaimedDelivery>> {
    let claim_token = Uuid::new_v4();
    let claim_for = pg_interval_seconds(DELIVERY_CLAIM_TTL.as_secs() as i64);
    let row = sqlx::query(
        &format!(
            r#"
            WITH candidate AS (
                SELECT id
                FROM webhook_deliveries
                WHERE next_attempt_at <= NOW()
                  AND (
                    status = 'pending'
                    OR (status = 'processing' AND (claim_expires_at IS NULL OR claim_expires_at <= NOW()))
                  )
                ORDER BY next_attempt_at ASC, id ASC
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            UPDATE webhook_deliveries d
            SET status = 'processing',
                claim_token = $1,
                claimed_at = NOW(),
                claim_expires_at = NOW() + {claim_for},
                updated_at = NOW()
            FROM candidate
            WHERE d.id = candidate.id
            RETURNING d.id, d.claim_token, d.target_url, d.headers, d.payload, d.attempts
            "#
        ),
    )
    .bind(claim_token)
    .fetch_optional(database)
    .await
    .context("Failed to claim webhook delivery")?;

    row.map(|row| {
        Ok(ClaimedDelivery {
            id: row.try_get("id")?,
            claim_token: row.try_get("claim_token")?,
            target_url: row.try_get("target_url")?,
            headers: parse_headers(row.try_get::<Value, _>("headers")?)?,
            payload: row.try_get("payload")?,
            attempts: row.try_get("attempts")?,
        })
    })
    .transpose()
}

fn pg_interval_seconds(seconds: i64) -> String {
    format!("make_interval(secs => {seconds})")
}

#[derive(Debug)]
enum DeliveryAttemptOutcome {
    Delivered {
        http_status: Option<i32>,
    },
    Retryable {
        http_status: Option<i32>,
        error: String,
        next_attempt_at: DateTime<Utc>,
    },
    Failed {
        http_status: Option<i32>,
        error: String,
    },
}

async fn deliver_once(client: &Client, delivery: &ClaimedDelivery) -> DeliveryAttemptOutcome {
    let mut request = client.post(&delivery.target_url).json(&delivery.payload);
    for (name, value) in &delivery.headers {
        request = request.header(name, value);
    }

    match request.send().await {
        Ok(response) => classify_http_response(delivery.attempts + 1, response.status().as_u16()),
        Err(err) => classify_transport_error(delivery.attempts + 1, err),
    }
}

fn classify_http_response(attempt_number: i32, status: u16) -> DeliveryAttemptOutcome {
    let http_status = Some(status as i32);
    if (200..300).contains(&status) {
        return DeliveryAttemptOutcome::Delivered { http_status };
    }
    if (500..600).contains(&status) {
        if let Some(next_attempt_at) = next_retry_at(Utc::now(), attempt_number) {
            return DeliveryAttemptOutcome::Retryable {
                http_status,
                error: format!("Remote endpoint returned HTTP {status}"),
                next_attempt_at,
            };
        }
    }
    DeliveryAttemptOutcome::Failed {
        http_status,
        error: format!("Remote endpoint returned HTTP {status}"),
    }
}

fn classify_transport_error(attempt_number: i32, error: reqwest::Error) -> DeliveryAttemptOutcome {
    if error.is_timeout() || error.is_connect() || error.status().is_none() {
        if let Some(next_attempt_at) = next_retry_at(Utc::now(), attempt_number) {
            return DeliveryAttemptOutcome::Retryable {
                http_status: error.status().map(|value| value.as_u16() as i32),
                error: error.to_string(),
                next_attempt_at,
            };
        }
    }

    DeliveryAttemptOutcome::Failed {
        http_status: error.status().map(|value| value.as_u16() as i32),
        error: error.to_string(),
    }
}

fn next_retry_at(now: DateTime<Utc>, attempt_number: i32) -> Option<DateTime<Utc>> {
    if attempt_number >= MAX_DELIVERY_ATTEMPTS {
        return None;
    }
    let backoff_minutes = 1_i64 << ((attempt_number - 1).max(0) as u32);
    Some(now + chrono::Duration::minutes(backoff_minutes))
}

async fn finalize_delivery_attempt(
    database: &PgPool,
    delivery: &ClaimedDelivery,
    outcome: DeliveryAttemptOutcome,
) -> anyhow::Result<()> {
    match outcome {
        DeliveryAttemptOutcome::Delivered { http_status } => {
            sqlx::query(
                r#"
                UPDATE webhook_deliveries
                SET status = 'delivered',
                    attempts = attempts + 1,
                    delivered_at = NOW(),
                    last_attempt_at = NOW(),
                    last_http_status = $3,
                    last_error = NULL,
                    claim_token = NULL,
                    claimed_at = NULL,
                    claim_expires_at = NULL,
                    updated_at = NOW()
                WHERE id = $1 AND claim_token = $2
                "#,
            )
            .bind(delivery.id)
            .bind(delivery.claim_token)
            .bind(http_status)
            .execute(database)
            .await
            .with_context(|| format!("Failed to mark webhook delivery {} as delivered", delivery.id))?;
        }
        DeliveryAttemptOutcome::Retryable {
            http_status,
            error,
            next_attempt_at,
        } => {
            sqlx::query(
                r#"
                UPDATE webhook_deliveries
                SET status = 'pending',
                    attempts = attempts + 1,
                    next_attempt_at = $3,
                    last_attempt_at = NOW(),
                    last_http_status = $4,
                    last_error = $5,
                    claim_token = NULL,
                    claimed_at = NULL,
                    claim_expires_at = NULL,
                    updated_at = NOW()
                WHERE id = $1 AND claim_token = $2
                "#,
            )
            .bind(delivery.id)
            .bind(delivery.claim_token)
            .bind(next_attempt_at)
            .bind(http_status)
            .bind(error)
            .execute(database)
            .await
            .with_context(|| format!("Failed to reschedule webhook delivery {}", delivery.id))?;
        }
        DeliveryAttemptOutcome::Failed { http_status, error } => {
            sqlx::query(
                r#"
                UPDATE webhook_deliveries
                SET status = 'failed',
                    attempts = attempts + 1,
                    last_attempt_at = NOW(),
                    last_http_status = $3,
                    last_error = $4,
                    claim_token = NULL,
                    claimed_at = NULL,
                    claim_expires_at = NULL,
                    updated_at = NOW()
                WHERE id = $1 AND claim_token = $2
                "#,
            )
            .bind(delivery.id)
            .bind(delivery.claim_token)
            .bind(http_status)
            .bind(error)
            .execute(database)
            .await
            .with_context(|| format!("Failed to mark webhook delivery {} as failed", delivery.id))?;
        }
    }

    Ok(())
}

pub fn latest_delivery_summary(
    status: Option<WebhookDeliveryStatus>,
    last_attempt_at: Option<DateTime<Utc>>,
    delivered_at: Option<DateTime<Utc>>,
    last_http_status: Option<i32>,
    last_error: Option<String>,
) -> (Option<WebhookDeliveryStatus>, Option<DateTime<Utc>>, Option<i32>, Option<String>) {
    (
        status,
        delivered_at.or(last_attempt_at),
        last_http_status,
        last_error,
    )
}

#[cfg(test)]
mod tests;
