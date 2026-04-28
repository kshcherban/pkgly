use bytes::Bytes;
use chrono::{DateTime, FixedOffset};
use reqwest::{RequestBuilder, Response, StatusCode, Url, multipart};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub base_url: String,
    pub token: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: Url,
    token: Option<String>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("{0}")]
    Http(HttpError),
}

#[derive(Debug, Clone)]
pub struct HttpError {
    pub status: StatusCode,
    pub body: String,
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}: {}",
            self.status,
            redact_http_body(&self.body)
        )
    }
}

impl std::error::Error for HttpError {}

fn redact_http_body(body: &str) -> String {
    let mut output = Vec::new();
    let mut parts = body.split_whitespace().peekable();
    while let Some(part) = parts.next() {
        output.push(part.to_string());
        if part.eq_ignore_ascii_case("bearer")
            && let Some(token) = parts.next()
        {
            output.push(redact_secret(token));
        }
    }
    output.join(" ")
}

fn redact_secret(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 8 {
        return "***".to_string();
    }
    let prefix: String = chars.iter().take(4).collect();
    let suffix: String = chars[chars.len().saturating_sub(4)..].iter().collect();
    format!("{prefix}...{suffix}")
}

impl Client {
    /// Creates a new Pkgly API client.
    pub fn new(config: ClientConfig) -> Result<Self, Error> {
        let mut builder = reqwest::Client::builder();
        if let Some(user_agent) = config.user_agent {
            builder = builder.user_agent(user_agent);
        }
        let http = builder.build()?;
        let base_url = normalize_base_url(&config.base_url)?;
        Ok(Self {
            http,
            base_url,
            token: config.token,
        })
    }

    /// Returns the absolute URL for an `/api/*` route.
    pub fn api_url(&self, route: &str) -> Result<Url, Error> {
        self.join_prefixed("api", route)
    }

    /// Returns the absolute URL for a `/repositories/*` route.
    pub fn repository_url(&self, route: &str) -> Result<Url, Error> {
        self.join_prefixed("repositories", route)
    }

    fn join_prefixed(&self, prefix: &str, route: &str) -> Result<Url, Error> {
        let mut url = self.base_url.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| url::ParseError::RelativeUrlWithCannotBeABaseBase)?;
            segments.pop_if_empty();
            segments.push(prefix);
            for segment in route.trim_matches('/').split('/') {
                if !segment.is_empty() {
                    segments.push(segment);
                }
            }
        }
        Ok(url)
    }

    fn with_auth(&self, request: RequestBuilder) -> RequestBuilder {
        if let Some(token) = &self.token {
            request.bearer_auth(token)
        } else {
            request
        }
    }

    pub fn get(&self, route: &str) -> Result<RequestBuilder, Error> {
        Ok(self.with_auth(self.http.get(self.api_url(route)?)))
    }

    pub fn post(&self, route: &str) -> Result<RequestBuilder, Error> {
        Ok(self.with_auth(self.http.post(self.api_url(route)?)))
    }

    pub fn put(&self, route: &str) -> Result<RequestBuilder, Error> {
        Ok(self.with_auth(self.http.put(self.api_url(route)?)))
    }

    pub fn delete(&self, route: &str) -> Result<RequestBuilder, Error> {
        Ok(self.with_auth(self.http.delete(self.api_url(route)?)))
    }

    pub fn repository_request(
        &self,
        method: reqwest::Method,
        route: &str,
    ) -> Result<RequestBuilder, Error> {
        Ok(self.with_auth(self.http.request(method, self.repository_url(route)?)))
    }

    async fn send_json<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T, Error> {
        let response = request.send().await?;
        let response = status_or_error(response).await?;
        let body = response.text().await?;
        Ok(serde_json::from_str(&body)?)
    }

    async fn send_no_content(&self, request: RequestBuilder) -> Result<(), Error> {
        let response = request.send().await?;
        status_or_error(response).await?;
        Ok(())
    }

    pub async fn list_repositories(&self) -> Result<Vec<Repository>, Error> {
        self.send_json(self.get("repository/list")?).await
    }

    pub async fn get_repository(&self, repository_id: Uuid) -> Result<Repository, Error> {
        self.send_json(self.get(&format!("repository/{repository_id}"))?)
            .await
    }

    pub async fn find_repository_id(
        &self,
        storage: &str,
        repository: &str,
    ) -> Result<RepositoryIdResponse, Error> {
        self.send_json(self.get(&format!(
            "repository/find-id/{}/{}",
            encode_segment(storage),
            encode_segment(repository)
        ))?)
        .await
    }

    pub async fn create_repository(
        &self,
        repository_type: &str,
        request: &CreateRepositoryRequest,
    ) -> Result<Repository, Error> {
        self.send_json(
            self.post(&format!(
                "repository/new/{}",
                encode_segment(repository_type)
            ))?
            .json(request),
        )
        .await
    }

    pub async fn delete_repository(&self, repository_id: Uuid) -> Result<(), Error> {
        self.send_no_content(self.delete(&format!("repository/{repository_id}"))?)
            .await
    }

    pub async fn list_repository_configs(&self, repository_id: Uuid) -> Result<Vec<String>, Error> {
        self.send_json(self.get(&format!("repository/{repository_id}/configs"))?)
            .await
    }

    pub async fn get_repository_config(
        &self,
        repository_id: Uuid,
        key: &str,
    ) -> Result<Value, Error> {
        self.send_json(self.get(&format!(
            "repository/{repository_id}/config/{}",
            encode_segment(key)
        ))?)
        .await
    }

    pub async fn set_repository_config(
        &self,
        repository_id: Uuid,
        key: &str,
        value: &Value,
    ) -> Result<(), Error> {
        self.send_no_content(
            self.put(&format!(
                "repository/{repository_id}/config/{}",
                encode_segment(key)
            ))?
            .json(value),
        )
        .await
    }

    pub async fn list_storages(&self) -> Result<Vec<Storage>, Error> {
        self.send_json(self.get("storage/list")?).await
    }

    pub async fn get_storage(&self, storage_id: Uuid) -> Result<Storage, Error> {
        self.send_json(self.get(&format!("storage/{storage_id}"))?)
            .await
    }

    pub async fn create_storage(
        &self,
        storage_type: &str,
        request: &CreateStorageRequest,
    ) -> Result<Storage, Error> {
        self.send_json(
            self.post(&format!("storage/new/{}", encode_segment(storage_type)))?
                .json(request),
        )
        .await
    }

    pub async fn list_packages(
        &self,
        repository_id: Uuid,
        query: &PackageListQuery,
    ) -> Result<PackageListResponse, Error> {
        self.send_json(
            self.get(&format!("repository/{repository_id}/packages"))?
                .query(query),
        )
        .await
    }

    pub async fn delete_packages(
        &self,
        repository_id: Uuid,
        paths: &[String],
    ) -> Result<PackageDeleteResponse, Error> {
        let body = PackageDeleteRequest {
            paths: paths.to_vec(),
        };
        self.send_json(
            self.delete(&format!("repository/{repository_id}/packages"))?
                .json(&body),
        )
        .await
    }

    pub async fn search_packages(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<PackageSearchResult>, Error> {
        self.send_json(
            self.get("search/packages")?
                .query(&[("q", query), ("limit", &limit.to_string())]),
        )
        .await
    }

    pub async fn whoami(&self) -> Result<UserIdentity, Error> {
        self.send_json(self.get("user/whoami")?).await
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<LoginSession, Error> {
        let body = LoginRequest {
            email_or_username: username.to_string(),
            password: password.to_string(),
        };
        let response = self
            .http
            .post(self.api_url("user/login")?)
            .header(reqwest::header::USER_AGENT, "pkglyctl")
            .json(&body)
            .send()
            .await?;
        let response = status_or_error(response).await?;
        let session_cookie = response
            .headers()
            .get(reqwest::header::SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .and_then(session_cookie_value)
            .ok_or(Error::Http(HttpError {
                status: StatusCode::UNAUTHORIZED,
                body: "login response did not include a session cookie".to_string(),
            }))?;
        Ok(LoginSession { session_cookie })
    }

    pub async fn create_token_with_session(
        &self,
        session: &LoginSession,
        request: &CreateTokenRequest,
    ) -> Result<CreateTokenResponse, Error> {
        self.send_json(
            self.http
                .post(self.api_url("user/token/create")?)
                .header(reqwest::header::USER_AGENT, "pkglyctl")
                .header(
                    reqwest::header::COOKIE,
                    format!("session={}", session.session_cookie),
                )
                .json(request),
        )
        .await
    }

    pub async fn logout(&self) -> Result<(), Error> {
        self.send_no_content(self.post("user/logout")?).await
    }

    pub async fn download_repository_path(
        &self,
        storage: &str,
        repository: &str,
        path: &str,
    ) -> Result<Response, Error> {
        let route = format!(
            "{}/{}/{}",
            encode_segment(storage),
            encode_segment(repository),
            path.trim_start_matches('/')
        );
        let response = self
            .repository_request(reqwest::Method::GET, &route)?
            .send()
            .await?;
        status_or_error(response).await
    }

    pub async fn put_repository_bytes(
        &self,
        storage: &str,
        repository: &str,
        path: &str,
        bytes: Bytes,
    ) -> Result<(), Error> {
        let route = repository_route(storage, repository, path);
        self.send_no_content(
            self.repository_request(reqwest::Method::PUT, &route)?
                .body(bytes),
        )
        .await
    }

    pub async fn post_repository_multipart(
        &self,
        storage: &str,
        repository: &str,
        path: &str,
        form: multipart::Form,
    ) -> Result<(), Error> {
        let route = repository_route(storage, repository, path);
        self.send_no_content(
            self.repository_request(reqwest::Method::POST, &route)?
                .multipart(form),
        )
        .await
    }

    pub async fn put_repository_multipart(
        &self,
        storage: &str,
        repository: &str,
        path: &str,
        form: multipart::Form,
    ) -> Result<(), Error> {
        let route = repository_route(storage, repository, path);
        self.send_no_content(
            self.repository_request(reqwest::Method::PUT, &route)?
                .multipart(form),
        )
        .await
    }

    pub async fn post_repository_bytes(
        &self,
        storage: &str,
        repository: &str,
        path: &str,
        bytes: Bytes,
    ) -> Result<(), Error> {
        let route = repository_route(storage, repository, path);
        self.send_no_content(
            self.repository_request(reqwest::Method::POST, &route)?
                .body(bytes),
        )
        .await
    }
}

fn session_cookie_value(value: &str) -> Option<String> {
    for part in value.split(';') {
        let trimmed = part.trim();
        if let Some(session) = trimmed.strip_prefix("session=") {
            return Some(session.to_string());
        }
    }
    None
}

fn repository_route(storage: &str, repository: &str, path: &str) -> String {
    format!(
        "{}/{}/{}",
        encode_segment(storage),
        encode_segment(repository),
        path.trim_start_matches('/')
    )
}

fn normalize_base_url(value: &str) -> Result<Url, url::ParseError> {
    let mut base = value.to_string();
    if !base.ends_with('/') {
        base.push('/');
    }
    Url::parse(&base)
}

fn encode_segment(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(&mut encoded, "%{byte:02X}");
            }
        }
    }
    encoded
}

async fn status_or_error(response: Response) -> Result<Response, Error> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    Err(Error::Http(HttpError { status, body }))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Repository {
    pub id: Uuid,
    pub storage_id: Uuid,
    pub storage_name: String,
    pub name: String,
    pub repository_type: String,
    #[serde(default)]
    pub repository_kind: Option<String>,
    pub visibility: String,
    pub active: bool,
    pub updated_at: DateTime<FixedOffset>,
    pub created_at: DateTime<FixedOffset>,
    #[serde(default)]
    pub auth_enabled: bool,
    #[serde(default)]
    pub storage_usage_bytes: Option<i64>,
    #[serde(default)]
    pub storage_usage_updated_at: Option<DateTime<FixedOffset>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryIdResponse {
    pub repository_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateRepositoryRequest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_name: Option<String>,
    pub configs: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Storage {
    pub id: Uuid,
    pub storage_type: String,
    pub name: String,
    #[serde(default)]
    pub config: Option<Value>,
    pub active: bool,
    pub updated_at: DateTime<FixedOffset>,
    pub created_at: DateTime<FixedOffset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateStorageRequest {
    pub name: String,
    pub config: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageListQuery {
    pub page: usize,
    pub per_page: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    pub sort_by: PackageSortBy,
    pub sort_dir: PackageSortDirection,
}

impl Default for PackageListQuery {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 50,
            q: None,
            sort_by: PackageSortBy::Modified,
            sort_dir: PackageSortDirection::Desc,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PackageSortBy {
    Modified,
    Package,
    Name,
    Size,
    Path,
    Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PackageSortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackageListResponse {
    pub page: usize,
    pub per_page: usize,
    pub total_packages: usize,
    pub items: Vec<PackageFileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackageFileEntry {
    pub package: String,
    pub name: String,
    pub cache_path: String,
    #[serde(default)]
    pub blob_digest: Option<String>,
    pub size: u64,
    pub modified: DateTime<FixedOffset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageDeleteRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageDeleteResponse {
    pub deleted: usize,
    pub missing: Vec<String>,
    pub rejected: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackageSearchResult {
    pub repository_id: Uuid,
    pub repository_name: String,
    pub storage_name: String,
    pub repository_type: String,
    pub file_name: String,
    pub cache_path: String,
    pub size: u64,
    pub modified: DateTime<FixedOffset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoginRequest {
    pub email_or_username: String,
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginSession {
    pub session_cookie: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateTokenRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub expires_in_days: Option<Value>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub repository_scopes: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateTokenResponse {
    pub id: i32,
    pub token: String,
    #[serde(default)]
    pub expires_at: Option<DateTime<FixedOffset>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserIdentity {
    pub id: i32,
    pub name: String,
    pub username: String,
    pub email: String,
    #[serde(default)]
    pub permissions: Option<Value>,
}

pub type NrApiError = Error;
pub type NrApi = Client;
