// ABOUTME: Verifies API authentication intersects real user permissions and token scopes.
// ABOUTME: Covers optional authentication and repository-specific scopes using PostgreSQL.
use crate::app::authentication::{Authentication, session::Session};
use nr_core::{
    database::{
        entities::user::{
            UserSafeData,
            auth_token::{AuthToken, NewAuthToken},
        },
        migration::run_migrations,
    },
    user::{
        permissions::{HasPermissions, RepositoryActions},
        scopes::NRScope,
    },
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::{clients::Cli, images::generic::GenericImage};
use uuid::Uuid;

async fn connect(url: &str) -> PgPool {
    for _ in 0..60 {
        if let Ok(pool) = PgPoolOptions::new().max_connections(3).connect(url).await {
            return pool;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    panic!("PostgreSQL did not become ready");
}

async fn repositories(pool: &PgPool) -> [Uuid; 2] {
    let storage = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO storages (id,name,storage_type,config) VALUES ($1,'scopes','Local',$2)",
    )
    .bind(storage)
    .bind(serde_json::json!({"type":"Local","settings":{"path":"/tmp/pkgly-scope-test"}}))
    .execute(pool)
    .await
    .expect("storage");
    let ids = [Uuid::new_v4(), Uuid::new_v4()];
    for (index, id) in ids.iter().enumerate() {
        sqlx::query("INSERT INTO repositories (id,storage_id,name,repository_type,visibility) VALUES ($1,$2,$3,'maven','Private')")
            .bind(id).bind(storage).bind(format!("repo-{index}")).execute(pool).await.expect("repository");
    }
    ids
}

async fn token(
    pool: &PgPool,
    user: &UserSafeData,
    scopes: Vec<NRScope>,
    repositories: Vec<(Uuid, Vec<RepositoryActions>)>,
) -> AuthToken {
    let (_, raw) = NewAuthToken {
        user_id: user.id,
        source: "scope-test".into(),
        scopes,
        repositories,
        ..Default::default()
    }
    .insert(pool)
    .await
    .expect("create token");
    AuthToken::get_by_token(&raw, pool)
        .await
        .expect("load token")
        .expect("token exists")
}

#[tokio::test]
async fn repository_scopes_apply_to_admin_tokens_and_optional_authentication() {
    let _guard = crate::test_support::DB_TEST_LOCK.lock().await;
    let docker = Cli::default();
    let container = docker.run(
        GenericImage::new("postgres", "18-alpine").with_env_var("POSTGRES_PASSWORD", "password"),
    );
    let pool = connect(&format!(
        "postgres://postgres:password@127.0.0.1:{}/postgres",
        container.get_host_port_ipv4(5432)
    ))
    .await;
    run_migrations(&pool).await.expect("migrations");
    let user: UserSafeData = sqlx::query_as("INSERT INTO users (name,username,active,admin) VALUES ('Scope test','mcp_scopes',true,true) RETURNING *")
        .fetch_one(&pool).await.expect("user");
    let [repo_a, repo_b] = repositories(&pool).await;
    let global_read = token(&pool, &user, vec![NRScope::ReadRepository], vec![]).await;
    let local_edit = token(
        &pool,
        &user,
        vec![],
        vec![(
            repo_a,
            vec![RepositoryActions::Read, RepositoryActions::Edit],
        )],
    )
    .await;
    let local_write = token(
        &pool,
        &user,
        vec![],
        vec![(repo_a, vec![RepositoryActions::Write])],
    )
    .await;
    for (token, action, repository, allowed) in [
        (global_read.clone(), RepositoryActions::Read, repo_b, true),
        (global_read, RepositoryActions::Edit, repo_a, false),
        (local_edit.clone(), RepositoryActions::Read, repo_a, true),
        (local_edit.clone(), RepositoryActions::Edit, repo_a, true),
        (local_edit.clone(), RepositoryActions::Read, repo_b, false),
        (local_edit, RepositoryActions::Write, repo_a, false),
        (local_write.clone(), RepositoryActions::Write, repo_a, true),
        (local_write, RepositoryActions::Read, repo_a, false),
    ] {
        let repository_auth =
            crate::repository::RepositoryAuthentication::AuthToken(token.clone(), user.clone());
        assert_eq!(
            repository_auth
                .has_action(action, repository, &pool)
                .await
                .expect("artifact action"),
            allowed,
            "artifact auth: {action:?} on {repository}"
        );
        let auth = Authentication::AuthToken(token, user.clone());
        assert_eq!(
            auth.has_action(action, repository, &pool)
                .await
                .expect("check action"),
            allowed,
            "direct auth: {action:?} on {repository}"
        );
        assert_eq!(
            Some(auth)
                .has_action(action, repository, &pool)
                .await
                .expect("optional action"),
            allowed,
            "optional auth: {action:?} on {repository}"
        );
    }
    verify_session_and_user_intersection(&pool, &user, repo_a).await;
    pool.close().await;
}

async fn verify_session_and_user_intersection(
    pool: &PgPool,
    admin: &UserSafeData,
    repository: Uuid,
) {
    let session = Session::new(
        admin.id,
        "test-session".into(),
        "tests".into(),
        "127.0.0.1".into(),
        chrono::Duration::hours(1),
    );
    let auth = Authentication::Session(session, admin.clone());
    assert!(
        Some(auth)
            .has_action(RepositoryActions::Edit, repository, pool)
            .await
            .expect("session permission")
    );
    let full_token = token(
        pool,
        admin,
        vec![
            NRScope::ReadRepository,
            NRScope::WriteRepository,
            NRScope::EditRepository,
        ],
        vec![],
    )
    .await;
    let restricted: UserSafeData = sqlx::query_as(
        "UPDATE users SET admin=false, default_repository_actions='{}' WHERE id=$1 RETURNING *",
    )
    .bind(admin.id)
    .fetch_one(pool)
    .await
    .expect("restrict user");
    let auth = Authentication::AuthToken(full_token, restricted);
    assert!(
        !auth
            .has_action(RepositoryActions::Read, repository, pool)
            .await
            .expect("user restriction")
    );
    let guest: Option<Authentication> = None;
    assert!(
        !guest
            .has_action(RepositoryActions::Read, repository, pool)
            .await
            .expect("guest restriction")
    );
}
