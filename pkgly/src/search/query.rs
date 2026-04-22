//! Database-backed package search over the `package_files` catalog.
//!
//! This module owns the low-level SQL used by the search API
//! (`app::api::search`) to fetch packages for a single repository.
//! Callers supply a `SearchQuery` and repository id; results are
//! ordered by most recently updated version and instrumented with
//! simple OpenTelemetry metrics.

use std::time::Instant;

use chrono::{DateTime, FixedOffset};
use once_cell::sync::Lazy;
use opentelemetry::{
    global,
    metrics::{Histogram, Meter},
};
use serde_json::Value;
use sqlx::{FromRow, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::app::api::search::{Operator, SearchQuery};

#[derive(Debug, Clone, FromRow)]
pub struct DatabasePackageRow {
    pub package_name: String,
    pub package_key: String,
    pub version: String,
    pub path: String,
    #[sqlx(json)]
    pub extra: Option<Value>,
    pub updated_at: DateTime<FixedOffset>,
}

pub struct PackageSearchRepository<'a> {
    database: &'a sqlx::PgPool,
}

impl<'a> PackageSearchRepository<'a> {
    pub fn new(database: &'a sqlx::PgPool) -> Self {
        Self { database }
    }

    pub async fn fetch_repository_rows(
        &self,
        repository_id: Uuid,
        query: &SearchQuery,
        limit: usize,
    ) -> Result<Vec<DatabasePackageRow>, sqlx::Error> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let fetch_limit = limit.max(1).min(500);
        let mut builder = QueryBuilder::<Postgres>::new(
            r#"
                SELECT
                    pf.package AS package_name,
                    pf.package AS package_key,
                    COALESCE(pv.version, pf.name) AS version,
                    pf.path,
                    jsonb_build_object(
                        'size', GREATEST(pf.size_bytes, 0),
                        'content_digest', pf.content_digest,
                        'upstream_digest', pf.upstream_digest
                    ) AS extra,
                    pf.modified_at AS updated_at
                FROM package_files pf
                LEFT JOIN project_versions pv ON pv.id = pf.project_version_id
                WHERE pf.repository_id =
            "#,
        );
        builder.push_bind(repository_id);
        builder.push(" AND pf.deleted_at IS NULL");

        if let Some((operator, filter)) = &query.package_filter {
            let normalized = filter.to_lowercase();
            match operator {
                Operator::Equals => {
                    builder.push(" AND (LOWER(pf.package) = ");
                    builder.push_bind(normalized.clone());
                    builder.push(" OR LOWER(pf.name) = ");
                    builder.push_bind(normalized);
                    builder.push(")");
                }
                Operator::Contains => {
                    let pattern = format!("%{normalized}%");
                    builder.push(" AND (");
                    push_collated_lower(&mut builder, "pf.package");
                    builder.push(" LIKE ");
                    builder.push_bind(pattern.clone());
                    builder.push(" OR ");
                    push_collated_lower(&mut builder, "pf.name");
                    builder.push(" LIKE ");
                    builder.push_bind(pattern);
                    builder.push(")");
                }
                _ => {}
            }
        }

        if let Some((operator, digest)) = &query.digest_filter {
            let normalized = digest.to_lowercase();
            match operator {
                Operator::Equals => {
                    builder
                        .push(" AND LOWER(COALESCE(pf.content_digest, pf.upstream_digest, '')) = ");
                    builder.push_bind(normalized);
                }
                Operator::Contains => {
                    builder.push(
                        " AND LOWER(COALESCE(pf.content_digest, pf.upstream_digest, '')) LIKE ",
                    );
                    builder.push_bind(format!("%{normalized}%"));
                }
                _ => {}
            }
        }

        for term in &query.terms {
            if term.is_empty() {
                continue;
            }
            let lowered = term.to_lowercase();
            let pattern = format!("%{lowered}%");
            builder.push(" AND (");
            push_collated_lower(&mut builder, "pf.package");
            builder.push(" LIKE ");
            builder.push_bind(pattern.clone());
            builder.push(" OR ");
            push_collated_lower(&mut builder, "pf.name");
            builder.push(" LIKE ");
            builder.push_bind(pattern.clone());
            builder.push(" OR ");
            push_collated_lower(&mut builder, "COALESCE(pv.version, pf.name)");
            builder.push(" LIKE ");
            builder.push_bind(pattern.clone());
            builder.push(" OR ");
            push_collated_lower(&mut builder, "pf.path");
            builder.push(" LIKE ");
            builder.push_bind(pattern.clone());
            builder.push(" OR ");
            push_collated_lower(
                &mut builder,
                "COALESCE(pf.content_digest, pf.upstream_digest, '')",
            );
            builder.push(" LIKE ");
            builder.push_bind(pattern);
            builder.push(")");
        }

        builder.push(" ORDER BY pf.modified_at DESC LIMIT ");
        builder.push_bind(fetch_limit as i64);

        let start = Instant::now();
        let rows = builder
            .build_query_as::<DatabasePackageRow>()
            .fetch_all(self.database)
            .await?;
        SEARCH_METRICS.record(start.elapsed().as_secs_f64() * 1_000.0, rows.len() as u64);
        Ok(rows)
    }

    pub async fn repository_has_index_rows(
        &self,
        repository_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM package_files WHERE repository_id = $1 AND deleted_at IS NULL)",
        )
        .bind(repository_id)
        .fetch_one(self.database)
        .await?;
        Ok(exists)
    }
}

fn push_collated_lower(builder: &mut QueryBuilder<Postgres>, expression: &str) {
    builder.push("LOWER(");
    builder.push(expression);
    builder.push(") COLLATE \"C\"");
}

struct SearchMetrics {
    #[allow(dead_code)]
    meter: Meter,
    query_duration_ms: Histogram<f64>,
    rows_returned: Histogram<u64>,
}

impl SearchMetrics {
    fn new() -> Self {
        let meter = global::meter("pkgly::search");
        let query_duration_ms = meter
            .f64_histogram("search.query.duration_ms")
            .with_description("Latency for DB-backed search queries")
            .with_unit("ms")
            .build();
        let rows_returned = meter
            .u64_histogram("search.query.rows")
            .with_description("Rows returned per repository search query")
            .build();
        Self {
            meter,
            query_duration_ms,
            rows_returned,
        }
    }

    fn record(&self, duration_ms: f64, rows: u64) {
        self.query_duration_ms.record(duration_ms, &[]);
        self.rows_returned.record(rows, &[]);
    }
}

static SEARCH_METRICS: Lazy<SearchMetrics> = Lazy::new(SearchMetrics::new);

#[cfg(test)]
mod tests;
