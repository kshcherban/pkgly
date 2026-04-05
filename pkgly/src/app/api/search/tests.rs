#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]

use std::{
    collections::{HashMap, HashSet},
    iter::FromIterator,
};

use async_trait::async_trait;
use chrono::{FixedOffset, TimeZone, Utc};

use super::{
    RepositorySummary,
    database::SearchBackend,
    execute_repository_search,
    query_parser::{Operator, SearchQuery},
};
use crate::search::query::DatabasePackageRow;
use uuid::Uuid;

struct StubBackend {
    rows: HashMap<Uuid, Vec<DatabasePackageRow>>,
    indexed: HashSet<Uuid>,
}

#[async_trait]
impl SearchBackend for StubBackend {
    async fn fetch_repository_rows(
        &self,
        repository_id: Uuid,
        _query: &SearchQuery,
        _limit: usize,
    ) -> Result<Vec<DatabasePackageRow>, sqlx::Error> {
        Ok(self.rows.get(&repository_id).cloned().unwrap_or_default())
    }

    async fn repository_has_index_rows(&self, repository_id: Uuid) -> Result<bool, sqlx::Error> {
        Ok(self.indexed.contains(&repository_id))
    }
}

fn deb_row(name: &str, version: &str) -> DatabasePackageRow {
    DatabasePackageRow {
        package_name: name.to_string(),
        package_key: name.to_string(),
        version: version.to_string(),
        path: format!("{name}/{version}/artifact.tgz"),
        extra: None,
        updated_at: Utc
            .with_ymd_and_hms(2025, 1, 1, 0, 0, 0)
            .single()
            .unwrap()
            .with_timezone(&FixedOffset::east_opt(0).unwrap()),
    }
}

fn summary(id: Uuid, name: &str, repo_type: &str) -> RepositorySummary {
    RepositorySummary {
        repository_id: id,
        repository_name: name.to_string(),
        storage_name: "primary".into(),
        repository_type: repo_type.to_string(),
    }
}

#[tokio::test]
async fn execute_repository_search_gathers_results() {
    let repo_a = Uuid::new_v4();
    let repo_b = Uuid::new_v4();
    let backend = StubBackend {
        rows: HashMap::from([
            (repo_a, vec![deb_row("pkg-a", "1.0.0")]),
            (repo_b, vec![deb_row("pkg-b", "2.0.0")]),
        ]),
        indexed: HashSet::from_iter([repo_a, repo_b]),
    };
    let summaries = vec![
        summary(repo_a, "alpha", "npm"),
        summary(repo_b, "bravo", "npm"),
    ];

    let outcome = execute_repository_search(&backend, &summaries, &SearchQuery::default(), 10)
        .await
        .expect("search");

    assert_eq!(outcome.results.len(), 2);
    assert!(outcome.unindexed.is_empty());
    assert_eq!(outcome.results[0].repository_name, "alpha");
}

#[tokio::test]
async fn execute_repository_search_applies_limit() {
    let repo_a = Uuid::new_v4();
    let repo_b = Uuid::new_v4();
    let backend = StubBackend {
        rows: HashMap::from([
            (repo_a, vec![deb_row("pkg-a", "1.0.0")]),
            (repo_b, vec![deb_row("pkg-b", "2.0.0")]),
        ]),
        indexed: HashSet::from_iter([repo_a, repo_b]),
    };
    let summaries = vec![
        summary(repo_a, "alpha", "npm"),
        summary(repo_b, "bravo", "npm"),
    ];

    let outcome = execute_repository_search(&backend, &summaries, &SearchQuery::default(), 1)
        .await
        .expect("search");

    assert_eq!(outcome.results.len(), 1);
}

#[tokio::test]
async fn execute_repository_search_marks_unindexed_repositories() {
    let repo = Uuid::new_v4();
    let backend = StubBackend {
        rows: HashMap::from([(repo, Vec::new())]),
        indexed: HashSet::new(),
    };
    let summaries = vec![summary(repo, "gamma", "docker")];

    let outcome = execute_repository_search(&backend, &summaries, &SearchQuery::default(), 5)
        .await
        .expect("search");

    assert!(outcome.results.is_empty());
    assert_eq!(outcome.unindexed, vec!["gamma".to_string()]);
}

#[tokio::test]
async fn execute_repository_search_respects_repository_filters() {
    let repo_a = Uuid::new_v4();
    let repo_b = Uuid::new_v4();
    let backend = StubBackend {
        rows: HashMap::from([
            (repo_a, vec![deb_row("pkg-a", "1.0.0")]),
            (repo_b, vec![deb_row("pkg-b", "2.0.0")]),
        ]),
        indexed: HashSet::from_iter([repo_a, repo_b]),
    };
    let summaries = vec![
        summary(repo_a, "alpha", "npm"),
        summary(repo_b, "bravo", "docker"),
    ];
    let query = SearchQuery {
        repository_filter: Some("alpha".into()),
        package_filter: Some((Operator::Equals, "pkg-a".into())),
        ..SearchQuery::default()
    };

    let outcome = execute_repository_search(&backend, &summaries, &query, 10)
        .await
        .expect("search");

    assert_eq!(outcome.results.len(), 1);
    assert_eq!(outcome.results[0].repository_name, "alpha");
}
