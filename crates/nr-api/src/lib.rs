use std::sync::Arc;

use nr_core::{
    database::entities::repository::DBRepositoryWithStorageName, repository::browse::BrowseResponse,
};
use reqwest::Response;
use thiserror::Error;
use uuid::Uuid;
#[derive(Debug, Error)]
pub enum NrApiError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("Not Found")]
    NotFound,
}
pub struct NrApiInner {
    client: reqwest::Client,
    base_url: String,
}

impl NrApiInner {
    pub fn new(client: reqwest::Client, base_url: String) -> Self {
        Self { client, base_url }
    }

    pub fn api_route(&self, route: &str) -> String {
        if self.base_url.ends_with('/') {
            format!("{}api/{}", self.base_url, route)
        } else {
            format!("{}/api/{}", self.base_url, route)
        }
    }

    pub fn repository_route(&self, route: &str) -> String {
        if self.base_url.ends_with('/') {
            format!("{}repositories/{}", self.base_url, route)
        } else {
            format!("{}/repositories/{}", self.base_url, route)
        }
    }
    pub fn get(&self, route: &str) -> reqwest::RequestBuilder {
        self.client.get(self.api_route(route))
    }

    pub fn post(&self, route: &str) -> reqwest::RequestBuilder {
        self.client.post(self.api_route(route))
    }
}
#[derive(Clone)]
pub struct NrApi(pub Arc<NrApiInner>);

impl NrApi {
    pub fn new(client: reqwest::Client, base_url: String) -> Self {
        Self(Arc::new(NrApiInner::new(client, base_url)))
    }

    pub fn api_route(&self, route: &str) -> String {
        self.0.api_route(route)
    }
    pub async fn get_repositories(&self) -> Result<Vec<DBRepositoryWithStorageName>, NrApiError> {
        let res = self.0.get("repository/list").send().await?;
        let res = res.error_for_status()?;
        let body = res.text().await?;
        Ok(serde_json::from_str(&body)?)
    }
    pub async fn get_repository(
        &self,
        repo_id: Uuid,
    ) -> Result<Option<DBRepositoryWithStorageName>, NrApiError> {
        let res = self
            .0
            .get(&format!("repository/{}", repo_id))
            .send()
            .await?;
        let res = res.error_for_status()?;
        let body = res.text().await?;
        Ok(serde_json::from_str(&body)?)
    }
    pub async fn browse_repository(
        &self,
        repo_id: Uuid,
        path: &str,
    ) -> Result<BrowseResponse, NrApiError> {
        let res = self
            .0
            .get(&format!("repository/browse/{}/{path}", repo_id))
            .send()
            .await?;
        let res = res.error_for_status()?;
        let body = res.text().await?;
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn get_file(&self, repo_id: Uuid, path: &str) -> Result<Response, NrApiError> {
        let repository = self
            .get_repository(repo_id)
            .await?
            .ok_or(NrApiError::NotFound)?;
        let res = self
            .0
            .client
            .get(self.0.repository_route(&format!(
                "{}/{}{path}",
                repository.storage_name, repository.name
            )))
            .send()
            .await?;
        let res = res.error_for_status()?;
        Ok(res)
    }
}
