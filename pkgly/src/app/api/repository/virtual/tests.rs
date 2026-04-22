#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;

use crate::repository::NewRepository;
use crate::test_support::DB_TEST_LOCK;
use nr_core::{
    database::{entities::storage::NewDBStorage, migration::run_migrations},
    storage::StorageName,
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::{Container, clients::Cli, images::generic::GenericImage};
use uuid::Uuid;

struct TestDb {
    pool: PgPool,
    _container: Container<'static, GenericImage>,
    _docker: &'static Cli,
}

impl TestDb {
    fn pool(&self) -> &PgPool {
        &self.pool
    }
}

async fn start_postgres() -> TestDb {
    let docker: &'static Cli = Box::leak(Box::new(Cli::default()));
    let image = GenericImage::new("postgres", "18-alpine")
        .with_env_var("POSTGRES_PASSWORD", "password")
        .with_env_var("POSTGRES_USER", "postgres")
        .with_env_var("POSTGRES_DB", "postgres");
    let container = docker.run(image);
    let port = container.get_host_port_ipv4(5432);
    let url = format!("postgres://postgres:password@127.0.0.1:{port}/postgres");

    for _ in 0..60 {
        match PgPoolOptions::new().max_connections(4).connect(&url).await {
            Ok(pool) => {
                return TestDb {
                    pool,
                    _container: container,
                    _docker: docker,
                };
            }
            Err(_) => {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        }
    }

    panic!("postgres container did not become ready");
}

async fn fresh_db() -> TestDb {
    let db = start_postgres().await;
    run_migrations(db.pool()).await.expect("run migrations");
    db
}

async fn insert_local_storage(pool: &PgPool, root: &std::path::Path) -> Uuid {
    let storage = NewDBStorage::new(
        "Local".into(),
        StorageName::new("primary".to_string()).expect("storage name"),
        serde_json::json!({
            "type": "Local",
            "settings": {
                "path": root.to_string_lossy()
            }
        }),
    );
    storage
        .insert(pool)
        .await
        .expect("insert storage")
        .expect("storage row")
        .id
}

async fn insert_repo(pool: &PgPool, storage_id: Uuid, name: &str, repository_type: &str) -> Uuid {
    NewRepository {
        name: name.to_string(),
        uuid: Uuid::new_v4(),
        repository_type: repository_type.to_string(),
        configs: ahash::HashMap::default(),
    }
    .insert(storage_id, pool)
    .await
    .expect("insert repo")
    .id
}

#[test]
fn update_members_request_deserializes_name_only_references() {
    let request: UpdateMembersRequest = serde_json::from_value(serde_json::json!({
        "members": [
            {
                "repository_name": "python-hosted",
                "priority": 1,
                "enabled": true
            }
        ],
        "publish_to": "python-hosted"
    }))
    .expect("request should deserialize");

    assert_eq!(request.members.len(), 1);
    assert_eq!(request.members[0].repository_id, None);
    assert_eq!(request.members[0].repository_name, "python-hosted");
    assert_eq!(
        request.publish_to,
        Some(RepositoryReference::Name("python-hosted".to_string()))
    );
}

#[tokio::test]
async fn resolve_virtual_config_input_resolves_name_only_members_and_publish_target() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let storage_root = tempfile::tempdir().expect("tempdir");
    let storage_id = insert_local_storage(db.pool(), storage_root.path()).await;
    let hosted_id = insert_repo(db.pool(), storage_id, "python-hosted", "python").await;
    let proxy_id = insert_repo(db.pool(), storage_id, "python-proxy", "python").await;

    let config = resolve_virtual_config_input(
        storage_id,
        VirtualRepositoryConfigInput {
            member_repositories: vec![
                VirtualRepositoryMemberInput {
                    repository_id: None,
                    repository_name: "python-hosted".to_string(),
                    priority: 1,
                    enabled: true,
                },
                VirtualRepositoryMemberInput {
                    repository_id: None,
                    repository_name: "python-proxy".to_string(),
                    priority: 10,
                    enabled: true,
                },
            ],
            resolution_order: VirtualResolutionOrder::Priority,
            cache_ttl_seconds: 60,
            publish_to: Some(RepositoryReference::Name("python-hosted".to_string())),
        },
        db.pool(),
    )
    .await
    .expect("resolve config");

    assert_eq!(config.member_repositories.len(), 2);
    assert_eq!(config.member_repositories[0].repository_id, hosted_id);
    assert_eq!(config.member_repositories[1].repository_id, proxy_id);
    assert_eq!(config.publish_to, Some(hosted_id));
}

#[tokio::test]
async fn normalize_virtual_repository_request_value_rewrites_name_only_create_payload() {
    let _guard = DB_TEST_LOCK.lock().await;
    let db = fresh_db().await;
    let storage_root = tempfile::tempdir().expect("tempdir");
    let storage_id = insert_local_storage(db.pool(), storage_root.path()).await;
    let hosted_id = insert_repo(db.pool(), storage_id, "npm-hosted", "npm").await;
    let proxy_id = insert_repo(db.pool(), storage_id, "npm-proxy", "npm").await;

    let mut repository_config = serde_json::json!({
        "type": "Virtual",
        "config": {
            "member_repositories": [
                {
                    "repository_name": "npm-hosted",
                    "priority": 1,
                    "enabled": true
                },
                {
                    "repository_name": "npm-proxy",
                    "priority": 10,
                    "enabled": true
                }
            ],
            "resolution_order": "Priority",
            "cache_ttl_seconds": 60,
            "publish_to": "npm-hosted"
        }
    });

    normalize_virtual_repository_request_value(storage_id, &mut repository_config, db.pool())
        .await
        .expect("normalize config");

    let config: crate::repository::npm::r#virtual::NpmVirtualConfig = serde_json::from_value(
        repository_config
            .get("config")
            .cloned()
            .expect("config value"),
    )
    .expect("normalized config");

    assert_eq!(config.member_repositories.len(), 2);
    assert_eq!(config.member_repositories[0].repository_id, hosted_id);
    assert_eq!(config.member_repositories[1].repository_id, proxy_id);
    assert_eq!(config.publish_to, Some(hosted_id));
}
