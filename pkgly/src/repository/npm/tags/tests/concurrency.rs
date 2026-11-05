#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::time::Duration;

use ahash::HashMap;
use nr_core::database::migration::run_migrations;
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions, types::Json};
use testcontainers::clients::Cli;
use tokio::task::JoinHandle;

use super::super::{load_tags, remove_tag, save_tags};
use super::migration::{PROJECT, previous_database};
use crate::test_support::DB_TEST_LOCK;

enum Write {
    Set(&'static str, &'static str),
    Remove(&'static str),
}

#[tokio::test]
async fn concurrent_tag_additions_preserve_both_writes_and_latest() {
    let _guard = DB_TEST_LOCK.lock().await;
    let docker = Cli::default();
    let (_container, pool) = previous_database(&docker).await;
    run_migrations(&pool).await.unwrap();
    assert_race(
        &pool,
        Some(json!({"latest":"1.0.0"})),
        [
            Write::Set("beta", "2.0.0-beta.1"),
            Write::Set("next", "1.0.0"),
        ],
        json!({"latest":"1.0.0", "beta":"2.0.0-beta.1", "next":"1.0.0"}),
    )
    .await;
}

#[tokio::test]
async fn concurrent_legacy_tag_initialization_preserves_fallback_and_both_writes() {
    let _guard = DB_TEST_LOCK.lock().await;
    let docker = Cli::default();
    let (_container, pool) = previous_database(&docker).await;
    run_migrations(&pool).await.unwrap();
    assert_race(
        &pool,
        None,
        [
            Write::Set("beta", "2.0.0-beta.1"),
            Write::Set("next", "1.0.0"),
        ],
        json!({"latest":"1.0.0", "beta":"2.0.0-beta.1", "next":"1.0.0"}),
    )
    .await;
}

#[tokio::test]
async fn concurrent_tag_removal_and_addition_do_not_restore_deleted_tags() {
    let _guard = DB_TEST_LOCK.lock().await;
    let docker = Cli::default();
    let (_container, pool) = previous_database(&docker).await;
    run_migrations(&pool).await.unwrap();
    assert_race(
        &pool,
        Some(json!({"latest":"1.0.0", "beta":"2.0.0-beta.1"})),
        [Write::Remove("beta"), Write::Set("next", "1.0.0")],
        json!({"latest":"1.0.0", "next":"1.0.0"}),
    )
    .await;
}

#[tokio::test]
async fn concurrent_updates_to_one_tag_keep_the_last_serialized_write() {
    let _guard = DB_TEST_LOCK.lock().await;
    let docker = Cli::default();
    let (_container, pool) = previous_database(&docker).await;
    run_migrations(&pool).await.unwrap();
    assert_race(
        &pool,
        Some(json!({"latest":"1.0.0", "next":"1.0.0"})),
        [
            Write::Set("beta", "1.0.0"),
            Write::Set("beta", "2.0.0-beta.1"),
        ],
        json!({"latest":"1.0.0", "next":"1.0.0", "beta":"2.0.0-beta.1"}),
    )
    .await;
}

async fn assert_race(pool: &PgPool, initial: Option<Value>, writes: [Write; 2], expected: Value) {
    sqlx::query("UPDATE projects SET npm_dist_tags = $2 WHERE id = $1")
        .bind(PROJECT)
        .bind(initial.map(Json))
        .execute(pool)
        .await
        .unwrap();
    let fallback = load_tags(PROJECT, pool).await.unwrap();
    let first_pool = worker_pool(pool, "npm-tag-race-a").await;
    let second_pool = worker_pool(pool, "npm-tag-race-b").await;
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM projects WHERE id = $1 FOR UPDATE")
        .bind(PROJECT)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let [first, second] = writes;
    let first = start_write(first_pool.clone(), fallback.clone(), first);
    wait_for_blocked_writes(pool, 1).await;
    let second = start_write(second_pool.clone(), fallback, second);
    wait_for_blocked_writes(pool, 2).await;
    blocker.commit().await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
    })
    .await
    .expect("both database writes finish after the lock is released");
    assert_eq!(
        serde_json::to_value(load_tags(PROJECT, pool).await.unwrap()).unwrap(),
        expected
    );
    first_pool.close().await;
    second_pool.close().await;
}

async fn worker_pool(pool: &PgPool, name: &str) -> PgPool {
    let options = (*pool.connect_options()).clone().application_name(name);
    PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap()
}

fn start_write(
    pool: PgPool,
    fallback: HashMap<String, String>,
    write: Write,
) -> JoinHandle<Result<(), super::super::super::NPMRegistryError>> {
    tokio::spawn(async move {
        match write {
            Write::Set(tag, version) => {
                save_tags(
                    PROJECT,
                    HashMap::from_iter([(tag.into(), version.into())]),
                    fallback,
                    &pool,
                )
                .await
            }
            Write::Remove(tag) => remove_tag(PROJECT, tag, &pool).await,
        }
    })
}

async fn wait_for_blocked_writes(pool: &PgPool, expected: i64) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let waiting: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database()
                 AND application_name IN ('npm-tag-race-a', 'npm-tag-race-b')
                 AND state = 'active' AND wait_event_type = 'Lock'",
            )
            .fetch_one(pool)
            .await
            .unwrap();
            if waiting == expected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("both writes reach the held row lock");
}
