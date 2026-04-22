use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

use super::{InternalError, PackageSearchResult, RepositorySummary, query_parser::SearchQuery};
use crate::search::query::{DatabasePackageRow, PackageSearchRepository};

#[async_trait]
pub trait SearchBackend: Send + Sync {
    async fn fetch_repository_rows(
        &self,
        repository_id: Uuid,
        query: &SearchQuery,
        limit: usize,
    ) -> Result<Vec<DatabasePackageRow>, sqlx::Error>;

    async fn repository_has_index_rows(&self, repository_id: Uuid) -> Result<bool, sqlx::Error>;
}

#[async_trait]
impl<'a> SearchBackend for PackageSearchRepository<'a> {
    async fn fetch_repository_rows(
        &self,
        repository_id: Uuid,
        query: &SearchQuery,
        limit: usize,
    ) -> Result<Vec<DatabasePackageRow>, sqlx::Error> {
        self.fetch_repository_rows(repository_id, query, limit)
            .await
    }

    async fn repository_has_index_rows(&self, repository_id: Uuid) -> Result<bool, sqlx::Error> {
        self.repository_has_index_rows(repository_id).await
    }
}

pub async fn search_database_packages<B: SearchBackend + ?Sized>(
    searcher: &B,
    summary: &RepositorySummary,
    query: &SearchQuery,
    limit: usize,
) -> Result<Vec<PackageSearchResult>, InternalError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let rows = searcher
        .fetch_repository_rows(summary.repository_id, query, limit)
        .await?;

    filter_database_rows(summary, rows, query, limit)
}

pub fn filter_database_rows(
    summary: &RepositorySummary,
    rows: Vec<DatabasePackageRow>,
    query: &SearchQuery,
    limit: usize,
) -> Result<Vec<PackageSearchResult>, InternalError> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    for row in rows {
        if results.len() >= limit {
            break;
        }

        let name_refs: Vec<&str> = vec![row.package_name.as_str(), row.package_key.as_str()];
        if query.package_filter.is_some() && !query.matches_package_names(&name_refs) {
            continue;
        }

        let digest = row
            .extra
            .as_ref()
            .and_then(|value| {
                value
                    .get("content_digest")
                    .or_else(|| value.get("upstream_digest"))
            })
            .and_then(Value::as_str)
            .unwrap_or_default();
        let extra_terms = row.extra.as_ref().map(Value::to_string).unwrap_or_default();
        let term_fields: Vec<&str> = vec![
            row.package_name.as_str(),
            row.package_key.as_str(),
            row.version.as_str(),
            row.path.as_str(),
            digest,
            extra_terms.as_str(),
        ];

        if !query.matches_terms(&term_fields) {
            continue;
        }

        if !query.matches_version(row.version.as_str()) {
            continue;
        }

        let file_name = row
            .path
            .rsplit('/')
            .next()
            .filter(|segment| !segment.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("{}@{}", row.package_name, row.version));

        results.push(PackageSearchResult {
            repository_id: summary.repository_id,
            repository_name: summary.repository_name.clone(),
            storage_name: summary.storage_name.clone(),
            repository_type: summary.repository_type.clone(),
            file_name,
            cache_path: row.path,
            size: extract_size(&row.extra),
            modified: row.updated_at,
        });
    }

    Ok(results)
}

fn extract_size(extra: &Option<Value>) -> u64 {
    extra
        .as_ref()
        .and_then(|value| value.get("size"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
