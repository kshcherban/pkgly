use axum::{
    extract::{Query, State},
    response::Response,
    routing::get,
};
use chrono::{DateTime, FixedOffset};
use nr_core::{
    database::entities::repository::DBRepositoryWithStorageName, repository::Visibility,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

use crate::{
    app::Pkgly, error::InternalError, repository::Repository, search::PackageSearchRepository,
    utils::ResponseBuilder,
};

mod database;
pub(crate) mod query_parser;
mod version_constraint;

pub(crate) use query_parser::{Operator, SearchQuery};

use self::{database::SearchBackend, query_parser::parse_search_query};

#[derive(OpenApi)]
#[openapi(paths(search_packages), components(schemas(PackageSearchResult)))]
pub struct SearchApi;

pub fn search_routes() -> axum::Router<Pkgly> {
    axum::Router::new().route("/packages", get(search_packages))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct PackageSearchQuery {
    #[serde(alias = "query")]
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

const fn default_limit() -> usize {
    25
}

#[derive(Debug, Serialize, ToSchema)]
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

pub(crate) struct RepositorySummary {
    repository_id: Uuid,
    repository_name: String,
    storage_name: String,
    repository_type: String,
}

#[derive(Default)]
pub(crate) struct SearchOutcome {
    pub results: Vec<PackageSearchResult>,
    pub unindexed: Vec<String>,
}

pub(crate) async fn execute_repository_search<B: SearchBackend + ?Sized>(
    searcher: &B,
    summaries: &[RepositorySummary],
    query: &SearchQuery,
    limit: usize,
) -> Result<SearchOutcome, InternalError> {
    if limit == 0 {
        return Ok(SearchOutcome::default());
    }
    let mut outcome = SearchOutcome::default();

    for summary in summaries {
        if outcome.results.len() >= limit {
            break;
        }
        if !query.matches_repository(
            &summary.repository_name,
            &summary.storage_name,
            &summary.repository_type,
        ) {
            continue;
        }
        let remaining = limit.saturating_sub(outcome.results.len());
        let repo_results =
            database::search_database_packages(searcher, summary, query, remaining).await?;

        if repo_results.is_empty()
            && !searcher
                .repository_has_index_rows(summary.repository_id)
                .await?
        {
            outcome.unindexed.push(summary.repository_name.clone());
        }

        outcome.results.extend(repo_results);
    }

    Ok(outcome)
}

#[utoipa::path(
    get,
    path = "/packages",
    params(PackageSearchQuery),
    responses((status = 200, description = "Package search results", body = [PackageSearchResult])),
    tag = "search"
)]
async fn search_packages(
    State(site): State<Pkgly>,
    Query(params): Query<PackageSearchQuery>,
) -> Result<Response, InternalError> {
    let raw_query = params.q.trim();
    if raw_query.is_empty() {
        let empty: [PackageSearchResult; 0] = [];
        return Ok(ResponseBuilder::ok().json(&empty));
    }
    let parsed_query = match parse_search_query(raw_query) {
        Ok(query) => query,
        Err(err) => {
            return Ok(ResponseBuilder::bad_request().body(err.to_string()));
        }
    };
    if !parsed_query.has_filters() && !parsed_query.terms.iter().any(|term| term.len() >= 2) {
        let empty: [PackageSearchResult; 0] = [];
        return Ok(ResponseBuilder::ok().json(&empty));
    }
    let limit = params.limit.clamp(1, 200);
    let searcher = PackageSearchRepository::new(&site.database);
    let mut summaries = Vec::new();

    for (repository_id, repository) in site.loaded_repositories() {
        let Some(info) =
            DBRepositoryWithStorageName::get_by_id(repository_id, site.as_ref()).await?
        else {
            continue;
        };

        if matches!(info.visibility, Visibility::Hidden) {
            continue;
        }

        summaries.push(RepositorySummary {
            repository_id,
            repository_name: repository.name(),
            storage_name: info.storage_name.to_string(),
            repository_type: info.repository_type,
        });
    }

    let outcome = execute_repository_search(&searcher, &summaries, &parsed_query, limit).await?;

    let mut builder = ResponseBuilder::ok();
    if outcome.results.is_empty() && !outcome.unindexed.is_empty() {
        let message = format!(
            "Repositories awaiting indexing: {}",
            outcome.unindexed.join(", ")
        );
        builder = builder.header("X-Pkgly-Warning", message);
    }

    Ok(builder.json(&outcome.results))
}

#[cfg(test)]
mod tests;
