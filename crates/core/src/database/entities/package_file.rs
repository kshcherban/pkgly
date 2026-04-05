use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use serde_json::Value;
use sqlx::{FromRow, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::{
    database::prelude::*,
    repository::project::{
        CargoPackageMetadata, DebPackageMetadata, PhpPackageMetadata, PythonPackageMetadata,
        RubyPackageMetadata,
    },
};

use super::project::{
    DBProject, ProjectDBType,
    versions::DBProjectVersion,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct DBPackageFile {
    pub id: i64,
    pub repository_id: Uuid,
    pub project_id: Option<Uuid>,
    pub project_version_id: Option<Uuid>,
    pub package: String,
    pub name: String,
    pub path: String,
    pub size_bytes: i64,
    pub content_digest: Option<String>,
    pub upstream_digest: Option<String>,
    pub modified_at: DateTime<FixedOffset>,
    pub created_at: DateTime<FixedOffset>,
    pub updated_at: DateTime<FixedOffset>,
    pub deleted_at: Option<DateTime<FixedOffset>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PackageFileSortBy {
    #[default]
    Modified,
    Package,
    Name,
    Size,
    Path,
    Digest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDirection {
    Asc,
    #[default]
    Desc,
}

#[derive(Debug, Clone)]
pub struct PackageFileListParams {
    pub repository_id: Uuid,
    pub page: usize,
    pub per_page: usize,
    pub search: Option<String>,
    pub sort_by: PackageFileSortBy,
    pub sort_dir: SortDirection,
}

#[derive(Debug, Clone)]
pub struct PackageFileUpsertInput {
    pub repository_id: Uuid,
    pub project_id: Option<Uuid>,
    pub project_version_id: Option<Uuid>,
    pub package: String,
    pub name: String,
    pub path: String,
    pub size_bytes: i64,
    pub content_digest: Option<String>,
    pub upstream_digest: Option<String>,
    pub modified_at: DateTime<FixedOffset>,
}

impl DBPackageFile {
    pub async fn upsert_file_row(
        database: &PgPool,
        input: PackageFileUpsertInput,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO package_files (
                repository_id,
                project_id,
                project_version_id,
                package,
                name,
                path,
                size_bytes,
                content_digest,
                upstream_digest,
                modified_at,
                deleted_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NULL)
            ON CONFLICT (repository_id, path_ci)
            DO UPDATE SET
                project_id = EXCLUDED.project_id,
                project_version_id = EXCLUDED.project_version_id,
                package = EXCLUDED.package,
                name = EXCLUDED.name,
                path = EXCLUDED.path,
                size_bytes = EXCLUDED.size_bytes,
                content_digest = EXCLUDED.content_digest,
                upstream_digest = EXCLUDED.upstream_digest,
                modified_at = EXCLUDED.modified_at,
                updated_at = NOW(),
                deleted_at = NULL
            RETURNING
                id,
                repository_id,
                project_id,
                project_version_id,
                package,
                name,
                path,
                size_bytes,
                content_digest,
                upstream_digest,
                modified_at,
                created_at,
                updated_at,
                deleted_at
            "#,
        )
        .bind(input.repository_id)
        .bind(input.project_id)
        .bind(input.project_version_id)
        .bind(input.package)
        .bind(input.name)
        .bind(input.path)
        .bind(input.size_bytes.max(0))
        .bind(input.content_digest)
        .bind(input.upstream_digest)
        .bind(input.modified_at)
        .fetch_one(database)
        .await
    }

    pub async fn upsert_from_project_version(
        database: &PgPool,
        version: &DBProjectVersion,
    ) -> Result<Option<Self>, sqlx::Error> {
        let Some(project) = <DBProject as ProjectDBType>::find_by_id(version.project_id, database)
            .await?
        else {
            return Ok(None);
        };
        let input = build_upsert_input(&project, version);
        let row = Self::upsert_file_row(database, input).await?;
        Ok(Some(row))
    }

    pub async fn delete_by_path(
        database: &PgPool,
        repository_id: Uuid,
        path: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM package_files
            WHERE repository_id = $1
              AND LOWER(path) = LOWER($2)
            "#,
        )
        .bind(repository_id)
        .bind(path)
        .execute(database)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn delete_by_paths_batch(
        database: &PgPool,
        repository_id: Uuid,
        paths: &[String],
    ) -> Result<u64, sqlx::Error> {
        let mut deleted = 0u64;
        for path in paths {
            deleted += Self::delete_by_path(database, repository_id, path).await?;
        }
        Ok(deleted)
    }

    pub async fn soft_delete_by_paths(
        database: &PgPool,
        repository_id: Uuid,
        paths: &[String],
    ) -> Result<u64, sqlx::Error> {
        let mut updated = 0u64;
        for path in paths {
            let result = sqlx::query(
                r#"
                UPDATE package_files
                SET deleted_at = NOW(),
                    updated_at = NOW()
                WHERE repository_id = $1
                  AND LOWER(path) = LOWER($2)
                  AND deleted_at IS NULL
                "#,
            )
            .bind(repository_id)
            .bind(path)
            .execute(database)
            .await?;
            updated += result.rows_affected();
        }
        Ok(updated)
    }

    pub async fn repository_has_rows(
        database: &PgPool,
        repository_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM package_files
                WHERE repository_id = $1
                  AND deleted_at IS NULL
            )
            "#,
        )
        .bind(repository_id)
        .fetch_one(database)
        .await?;
        Ok(exists)
    }

    pub async fn list_repository_page(
        database: &PgPool,
        params: &PackageFileListParams,
    ) -> Result<(usize, Vec<Self>), sqlx::Error> {
        let per_page = params.per_page.max(1);
        let page = params.page.max(1);
        let offset = ((page - 1) * per_page) as i64;
        let search = normalize_search_term(params.search.as_deref());

        let total = query_total(database, params.repository_id, search.as_deref()).await?;
        let rows = query_page(database, params, search.as_deref(), offset).await?;

        Ok((total as usize, rows))
    }
}

fn normalize_search_term(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_lowercase())
}

async fn query_total(
    database: &PgPool,
    repository_id: Uuid,
    search: Option<&str>,
) -> Result<i64, sqlx::Error> {
    let mut builder = QueryBuilder::<Postgres>::new(
        r#"
        SELECT COUNT(*)
        FROM package_files
        WHERE repository_id =
        "#,
    );
    builder.push_bind(repository_id);
    builder.push(" AND deleted_at IS NULL");
    push_search_clause(&mut builder, search);

    builder.build_query_scalar().fetch_one(database).await
}

async fn query_page(
    database: &PgPool,
    params: &PackageFileListParams,
    search: Option<&str>,
    offset: i64,
) -> Result<Vec<DBPackageFile>, sqlx::Error> {
    let mut builder = QueryBuilder::<Postgres>::new(
        r#"
        SELECT
            id,
            repository_id,
            project_id,
            project_version_id,
            package,
            name,
            path,
            size_bytes,
            content_digest,
            upstream_digest,
            modified_at,
            created_at,
            updated_at,
            deleted_at
        FROM package_files
        WHERE repository_id =
        "#,
    );
    builder.push_bind(params.repository_id);
    builder.push(" AND deleted_at IS NULL");
    push_search_clause(&mut builder, search);

    let sort_expr = sort_expression(params.sort_by);
    let sort_dir = match params.sort_dir {
        SortDirection::Asc => "ASC",
        SortDirection::Desc => "DESC",
    };
    builder.push(" ORDER BY ");
    builder.push(sort_expr);
    builder.push(" ");
    builder.push(sort_dir);
    builder.push(", id ");
    builder.push(sort_dir);
    builder.push(" LIMIT ");
    builder.push_bind(params.per_page as i64);
    builder.push(" OFFSET ");
    builder.push_bind(offset);

    builder.build_query_as::<DBPackageFile>().fetch_all(database).await
}

fn push_search_clause(builder: &mut QueryBuilder<'_, Postgres>, search: Option<&str>) {
    let Some(search) = search else {
        return;
    };
    let pattern = format!("%{search}%");
    builder.push(
        r#"
        AND (
            LOWER(package) COLLATE "C" LIKE
        "#,
    );
    builder.push_bind(pattern.clone());
    builder.push(
        r#"
            OR LOWER(name) COLLATE "C" LIKE
        "#,
    );
    builder.push_bind(pattern.clone());
    builder.push(
        r#"
            OR LOWER(path) COLLATE "C" LIKE
        "#,
    );
    builder.push_bind(pattern.clone());
    builder.push(
        r#"
            OR LOWER(COALESCE(content_digest, upstream_digest, '')) COLLATE "C" LIKE
        "#,
    );
    builder.push_bind(pattern);
    builder.push(")");
}

fn sort_expression(sort_by: PackageFileSortBy) -> &'static str {
    match sort_by {
        PackageFileSortBy::Modified => "modified_at",
        PackageFileSortBy::Package => "LOWER(package) COLLATE \"C\"",
        PackageFileSortBy::Name => "LOWER(name) COLLATE \"C\"",
        PackageFileSortBy::Size => "size_bytes",
        PackageFileSortBy::Path => "LOWER(path) COLLATE \"C\"",
        PackageFileSortBy::Digest => "LOWER(COALESCE(content_digest, upstream_digest, '')) COLLATE \"C\"",
    }
}

fn build_upsert_input(project: &DBProject, version: &DBProjectVersion) -> PackageFileUpsertInput {
    if let Some(proxy_meta) = version.extra.0.proxy_artifact() {
        let path = proxy_meta.cache_path;
        let name = proxy_meta
            .version
            .clone()
            .unwrap_or_else(|| file_name_from_path(&path, &version.version));
        let modified_at: DateTime<FixedOffset> = proxy_meta.fetched_at.into();
        return PackageFileUpsertInput {
            repository_id: version.repository_id,
            project_id: Some(project.id),
            project_version_id: Some(version.id),
            package: proxy_meta.package_key,
            name,
            path,
            size_bytes: proxy_meta.size.unwrap_or_default() as i64,
            content_digest: None,
            upstream_digest: normalize_digest(proxy_meta.upstream_digest.as_deref()),
            modified_at,
        };
    }

    let mut path = version.path.clone();
    let mut name = file_name_from_path(&path, &version.version);
    let mut size_bytes = extract_size(version).unwrap_or_default() as i64;
    let mut content_digest = extract_digest(version);

    if let Some(metadata) = parse_extra::<DebPackageMetadata>(version) {
        path = metadata.filename;
        name = version.version.clone();
        size_bytes = metadata.size as i64;
        content_digest = normalize_digest(Some(metadata.sha256.as_str()));
    } else if let Some(metadata) = parse_extra::<CargoPackageMetadata>(version) {
        size_bytes = metadata.crate_size as i64;
        content_digest = normalize_digest(Some(metadata.checksum.as_str()));
    } else if let Some(metadata) = parse_extra::<PythonPackageMetadata>(version) {
        if metadata.filename.contains('/') {
            path = metadata.filename.clone();
        }
        name = metadata.filename;
        content_digest = normalize_digest(metadata.sha256.as_deref());
        size_bytes = extract_size(version).unwrap_or_default() as i64;
    } else if let Some(metadata) = parse_extra::<PhpPackageMetadata>(version) {
        if metadata.filename.contains('/') {
            path = metadata.filename.clone();
        }
        name = file_name_from_path(&path, &version.version);
        content_digest = normalize_digest(metadata.sha256.as_deref());
        size_bytes = extract_size(version).unwrap_or_default() as i64;
    } else if let Some(metadata) = parse_extra::<RubyPackageMetadata>(version) {
        if metadata.filename.contains('/') {
            path = metadata.filename.clone();
        }
        name = metadata.filename;
        content_digest = normalize_digest(metadata.sha256.as_deref());
    }

    PackageFileUpsertInput {
        repository_id: version.repository_id,
        project_id: Some(project.id),
        project_version_id: Some(version.id),
        package: project.key.clone(),
        name,
        path,
        size_bytes,
        content_digest,
        upstream_digest: None,
        modified_at: version.updated_at,
    }
}

fn parse_extra<T: DeserializeOwned>(version: &DBProjectVersion) -> Option<T> {
    let extra = version.extra.0.extra.as_ref()?;
    serde_json::from_value(extra.clone()).ok()
}

fn extract_size(version: &DBProjectVersion) -> Option<u64> {
    let extra = version.extra.0.extra.as_ref()?;
    for key in ["size", "crate_size", "size_bytes"] {
        if let Some(value) = extract_u64(extra.get(key)) {
            return Some(value);
        }
    }
    None
}

fn extract_u64(value: Option<&Value>) -> Option<u64> {
    let value = value?;
    if let Some(num) = value.as_u64() {
        return Some(num);
    }
    let str_value = value.as_str()?;
    str_value.parse().ok()
}

fn extract_digest(version: &DBProjectVersion) -> Option<String> {
    let extra = version.extra.0.extra.as_ref()?;
    for key in ["sha256", "checksum", "digest", "content_digest"] {
        if let Some(value) = extra.get(key).and_then(Value::as_str)
            && let Some(normalized) = normalize_digest(Some(value))
        {
            return Some(normalized);
        }
    }
    None
}

fn normalize_digest(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains(':') {
        return Some(trimmed.to_string());
    }
    Some(format!("sha256:{trimmed}"))
}

fn file_name_from_path(path: &str, fallback: &str) -> String {
    path.rsplit('/')
        .next()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(fallback)
        .to_string()
}

#[cfg(test)]
mod tests;
