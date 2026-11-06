#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
use super::*;
use crate::repository::{NewRepository, RepositoryAuthConfigType, npm::NPMRegistryConfigType};
use nr_core::database::entities::user::{
    UserSafeData,
    auth_token::{AuthToken, NewAuthToken},
};
use nr_core::user::scopes::NRScope;
use serde_json::json;

async fn npm_site(db: &TestDb, root: &std::path::Path) -> Pkgly {
    npm_site_with_upstream(db, root, "http://127.0.0.1:9").await
}

async fn npm_site_with_upstream(db: &TestDb, root: &std::path::Path, upstream: &str) -> Pkgly {
    let storage = NewDBStorage::new(
        "Local".into(),
        StorageName::new("npm-test".into()).unwrap(),
        json!({"type":"Local","settings":{"path":root.join("files")}}),
    )
    .insert(&db.pool)
    .await
    .unwrap()
    .unwrap();
    for (name, visibility, enabled) in [
        ("public", "Public", false),
        ("protected", "Public", true),
        ("private", "Private", false),
        ("hidden", "Hidden", false),
        ("proxy-protected", "Public", true),
        ("proxy-private", "Private", false),
    ] {
        let id = NewRepository {
            name: name.into(),
            uuid: Uuid::new_v4(),
            repository_type: "npm".into(),
            configs: ahash::HashMap::from_iter([
                (
                    NPMRegistryConfigType::get_type_static().into(),
                    if name.starts_with("proxy-") {
                        json!({"type":"Proxy","config":{"routes":[{"url":upstream}]}})
                    } else {
                        json!({"type":"Hosted"})
                    },
                ),
                (
                    RepositoryAuthConfigType::get_type_static().into(),
                    json!({"enabled":enabled}),
                ),
            ]),
        }
        .insert(storage.id, &db.pool)
        .await
        .unwrap();
        sqlx::query("UPDATE repositories SET visibility = $2 WHERE id = $1")
            .bind(id.id)
            .bind(visibility)
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO projects (repository_id, key, name, path, npm_dist_tags) VALUES ($1, 'demo', 'demo', 'demo', '{}')")
            .bind(id.id).execute(&db.pool).await.unwrap();
    }
    let target: Uuid = sqlx::query_scalar("SELECT id FROM repositories WHERE name='private'")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    for (name, enabled, publish_to) in [
        ("virtual", false, Some(target)),
        ("virtual-protected", true, Some(target)),
        ("virtual-private", false, Some(target)),
        ("virtual-no-target", false, None),
    ] {
        NewRepository { name:name.into(), uuid:Uuid::new_v4(), repository_type:"npm".into(),
            configs:ahash::HashMap::from_iter([
                (NPMRegistryConfigType::get_type_static().into(), json!({"type":"Virtual","config":{
                    "member_repositories":[{"repository_id":target,"repository_name":"private"}],"publish_to":publish_to}})),
                (RepositoryAuthConfigType::get_type_static().into(), json!({"enabled":enabled})),
            ])}.insert(storage.id,&db.pool).await.unwrap();
    }
    sqlx::query("UPDATE repositories SET visibility='Private' WHERE name='virtual-private'")
        .execute(&db.pool)
        .await
        .unwrap();
    Pkgly::new(
        Mode::Debug,
        SiteSetting::default(),
        SecuritySettings::default(),
        SessionManagerConfig {
            database_location: root.join("sessions.redb"),
            ..Default::default()
        },
        StagingConfig {
            staging_dir: root.join("staging"),
            ..Default::default()
        },
        None,
        DatabaseConfig {
            user: "postgres".into(),
            password: "password".into(),
            database: "postgres".into(),
            host: "127.0.0.1".into(),
            port: Some(db.port),
        },
        Some(root.join("files")),
    )
    .await
    .unwrap()
}

async fn npm_user(pool: &PgPool, name: &str, actions: Vec<RepositoryActions>) -> UserSafeData {
    NewUserRequest {
        name: name.into(),
        username: Username::new(name.into()).unwrap(),
        email: None,
        password: None,
        permissions: Some(InitialUserPermissions {
            default_repository_actions: actions,
            ..Default::default()
        }),
    }
    .insert(pool)
    .await
    .unwrap()
    .into()
}

async fn token_auth(
    pool: &PgPool,
    user: UserSafeData,
    scopes: Vec<NRScope>,
) -> RepositoryAuthentication {
    let (_, raw) = NewAuthToken {
        user_id: user.id,
        scopes,
        source: "test".into(),
        ..Default::default()
    }
    .insert(pool)
    .await
    .unwrap();
    let token = AuthToken::get_by_token(&raw, pool).await.unwrap().unwrap();
    RepositoryAuthentication::AuthToken(token, user)
}

async fn npm_request(
    site: &Pkgly,
    repo: &str,
    path: &str,
    method: Method,
    auth: RepositoryAuthentication,
) -> Response {
    let request = http::Request::builder()
        .method(method)
        .uri(format!("/repositories/npm-test/{repo}/{path}"))
        .header(
            "npm-command",
            if path == "demo-publish" {
                "publish"
            } else {
                "deprecate"
            },
        )
        .body(Body::from("{}"))
        .unwrap();
    handle_repo_request_core(
        site.clone(),
        RepoRequestPath {
            storage: "npm-test".into(),
            repository: repo.into(),
            path: Some(StoragePath::from(path)),
            docker_scope: None,
        },
        None,
        auth,
        request,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn npm_existing_reads_keep_upstream_authorization_policy() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    let no_read = npm_user(&db.pool, "npm-existing-reader", vec![]).await;
    let reader = npm_user(
        &db.pool,
        "npm-existing-proxy-reader",
        vec![RepositoryActions::Read],
    )
    .await;
    let identities = [
        RepositoryAuthentication::Basic(None, no_read.clone()),
        token_auth(&db.pool, no_read, vec![NRScope::WriteRepository]).await,
    ];
    let proxy_identities = [
        (
            token_auth(&db.pool, reader.clone(), vec![NRScope::ReadRepository]).await,
            true,
        ),
        (
            token_auth(&db.pool, reader, vec![NRScope::WriteRepository]).await,
            false,
        ),
    ];
    for path in ["demo", "demo/1.0.0", "demo/-/demo-1.0.0.tgz"] {
        for method in [Method::GET, Method::HEAD] {
            for auth in &identities {
                let response =
                    npm_request(&site, "protected", path, method.clone(), auth.clone()).await;
                assert!(
                    !matches!(response.status().as_u16(), 401 | 403),
                    "authenticated hosted {method} {path}: {}",
                    response.status()
                );
            }
            for repo in ["private", "hidden"] {
                let response = npm_request(
                    &site,
                    repo,
                    path,
                    method.clone(),
                    RepositoryAuthentication::NoIdentification,
                )
                .await;
                assert!(
                    !matches!(response.status().as_u16(), 401 | 403),
                    "existing hosted {repo} {method} {path}: {}",
                    response.status()
                );
            }
            for (auth, allowed) in &proxy_identities {
                let response =
                    npm_request(&site, "proxy-private", path, method.clone(), auth.clone()).await;
                assert_eq!(
                    !matches!(response.status().as_u16(), 401 | 403),
                    *allowed,
                    "upstream proxy read {method} {path}: {}",
                    response.status()
                );
            }
        }
    }
}

#[tokio::test]
async fn npm_protected_tag_reads_require_user_permission_and_token_scope() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    let reader = npm_user(&db.pool, "npm-tag-reader", vec![RepositoryActions::Read]).await;
    let denied = npm_user(&db.pool, "npm-tag-denied", vec![]).await;
    let allowed = [
        RepositoryAuthentication::Basic(None, reader.clone()),
        token_auth(&db.pool, reader.clone(), vec![NRScope::ReadRepository]).await,
    ];
    let rejected = [
        RepositoryAuthentication::NoIdentification,
        RepositoryAuthentication::Basic(None, denied.clone()),
        token_auth(&db.pool, reader, vec![NRScope::WriteRepository]).await,
        token_auth(&db.pool, denied, vec![NRScope::ReadRepository]).await,
    ];
    for repo in [
        "protected",
        "private",
        "hidden",
        "proxy-private",
        "virtual-private",
    ] {
        for path in ["-/package/demo/dist-tags", "-/package/demo/dist-tags/beta"] {
            for method in [Method::GET, Method::HEAD] {
                for auth in &rejected {
                    let response =
                        npm_request(&site, repo, path, method.clone(), auth.clone()).await;
                    assert!(
                        matches!(response.status().as_u16(), 401 | 403),
                        "{repo} {method} {path}: {}",
                        response.status()
                    );
                }
                for auth in &allowed {
                    let response =
                        npm_request(&site, repo, path, method.clone(), auth.clone()).await;
                    assert!(
                        !matches!(response.status().as_u16(), 401 | 403),
                        "authorized {repo} {method} {path}: {}",
                        response.status()
                    );
                }
            }
        }
    }
    for method in [Method::GET, Method::HEAD] {
        assert_eq!(
            npm_request(
                &site,
                "public",
                "-/package/demo/dist-tags",
                method,
                RepositoryAuthentication::NoIdentification
            )
            .await
            .status(),
            StatusCode::OK
        );
    }
}

#[tokio::test]
async fn npm_tag_changes_require_write_without_requiring_read_permission() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    let writer = npm_user(&db.pool, "npm-tag-writer", vec![RepositoryActions::Write]).await;
    let auth = token_auth(&db.pool, writer, vec![NRScope::WriteRepository]).await;
    sqlx::query("INSERT INTO project_versions (project_id,repository_id,version,release_type,path,extra) SELECT id,repository_id,'1.0.0','stable','demo/1.0.0','{\"extra\":{}}' FROM projects WHERE key='demo'").execute(&db.pool).await.unwrap();
    for (method, body) in [(Method::PUT, "\"1.0.0\""), (Method::DELETE, "")] {
        let request = http::Request::builder()
            .method(method.clone())
            .uri("/repositories/npm-test/protected/-/package/demo/dist-tags/beta")
            .body(Body::from(body))
            .unwrap();
        let response = handle_repo_request_core(
            site.clone(),
            RepoRequestPath {
                storage: "npm-test".into(),
                repository: "protected".into(),
                path: Some(StoragePath::from("-/package/demo/dist-tags/beta")),
                docker_scope: None,
            },
            None,
            auth.clone(),
            request,
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT, "{method}");
    }
}

#[tokio::test]
async fn npm_public_proxy_and_virtual_reads_allow_anonymous_access_when_auth_enabled() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    for repo in ["proxy-protected", "virtual-protected"] {
        for path in [
            "demo",
            "demo/1.0.0",
            "demo/-/demo-1.0.0.tgz",
            "-/package/demo/dist-tags",
        ] {
            for method in [Method::GET, Method::HEAD] {
                let response = npm_request(
                    &site,
                    repo,
                    path,
                    method.clone(),
                    RepositoryAuthentication::NoIdentification,
                )
                .await;
                assert!(
                    !matches!(response.status().as_u16(), 401 | 403),
                    "{repo} {method} {path}: {}",
                    response.status()
                );
            }
        }
    }
}

#[tokio::test]
async fn npm_all_write_operations_require_user_permission_and_token_scope() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    let writer = npm_user(&db.pool, "npm-writer", vec![RepositoryActions::Write]).await;
    let reader = npm_user(&db.pool, "npm-write-denied", vec![RepositoryActions::Read]).await;
    let denied = [
        RepositoryAuthentication::NoIdentification,
        RepositoryAuthentication::Basic(None, reader.clone()),
        token_auth(&db.pool, writer, vec![NRScope::ReadRepository]).await,
        token_auth(&db.pool, reader, vec![NRScope::WriteRepository]).await,
    ];
    for (method, path) in [
        (Method::PUT, "demo"),
        (Method::PUT, "demo-publish"),
        (Method::PUT, "-/package/demo/dist-tags/beta"),
        (Method::DELETE, "-/package/demo/dist-tags/beta"),
    ] {
        for auth in &denied {
            let response = npm_request(&site, "public", path, method.clone(), auth.clone()).await;
            assert!(
                matches!(response.status().as_u16(), 401 | 403),
                "{method} {path}: {}",
                response.status()
            );
        }
    }
    let tags: Vec<serde_json::Value> = sqlx::query_scalar("SELECT npm_dist_tags FROM projects")
        .fetch_all(&db.pool)
        .await
        .unwrap();
    assert!(tags.iter().all(|tags| *tags == json!({})));
}

#[tokio::test]
async fn npm_audit_records_operation_names_for_success_and_denial() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    for (method, path, command, action) in [
        (
            Method::GET,
            "-/package/demo/dist-tags",
            "",
            "package.dist_tag.list",
        ),
        (
            Method::PUT,
            "-/package/demo/dist-tags/beta",
            "",
            "package.dist_tag.set",
        ),
        (
            Method::DELETE,
            "-/package/demo/dist-tags/beta",
            "",
            "package.dist_tag.delete",
        ),
        (
            Method::PUT,
            "demo",
            "deprecate",
            "package.deprecation.update",
        ),
        (
            Method::PUT,
            "demo",
            "undeprecate",
            "package.deprecation.update",
        ),
        (Method::PUT, "demo", "publish", "package.upload"),
    ] {
        for repo in ["public", "protected"] {
            let ctx = AccessLogContext::default();
            let mut request = http::Request::builder()
                .method(method.clone())
                .uri(format!("/repositories/npm-test/{repo}/{path}"))
                .header("npm-command", command)
                .body(Body::empty())
                .unwrap();
            request.extensions_mut().insert(ctx.clone());
            handle_repo_request_core(
                site.clone(),
                RepoRequestPath {
                    storage: "npm-test".into(),
                    repository: repo.into(),
                    path: Some(StoragePath::from(path)),
                    docker_scope: None,
                },
                None,
                RepositoryAuthentication::NoIdentification,
                request,
            )
            .await
            .unwrap();
            assert_eq!(
                ctx.snapshot().audit_action.as_deref(),
                Some(action),
                "{repo} {method} {path}"
            );
        }
    }
}

#[tokio::test]
async fn npm_path_index_preserves_other_repository_uniqueness() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    let _ = site;
    let repo: Uuid = sqlx::query_scalar("SELECT id FROM repositories WHERE name='public'")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let project: Uuid = sqlx::query_scalar("SELECT id FROM projects WHERE repository_id=$1")
        .bind(repo)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE repositories SET repository_type='maven' WHERE id=$1")
        .bind(repo)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO project_versions (project_id,repository_id,version,release_type,path) VALUES ($1,$2,'1.0.0','stable','artifact/UPPER')")
        .bind(project).bind(repo).execute(&db.pool).await.unwrap();
    let duplicate = sqlx::query("INSERT INTO project_versions (project_id,repository_id,version,release_type,path) VALUES ($1,$2,'2.0.0','stable','artifact/upper')")
        .bind(project).bind(repo).execute(&db.pool).await;
    assert!(
        duplicate.is_err(),
        "non-npm paths retain case-insensitive uniqueness"
    );
}

mod logging;

#[tokio::test]
async fn npm_virtual_tag_changes_require_target_write_permission() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    let user = npm_user(&db.pool, "npm-virtual-reader", vec![]).await;
    let virtual_id: Uuid =
        sqlx::query_scalar("SELECT id FROM repositories WHERE name='virtual-protected'")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let target: Uuid = sqlx::query_scalar("SELECT id FROM repositories WHERE name='private'")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    NewUserRepositoryPermissions {
        user_id: user.id,
        repository_id: virtual_id,
        actions: vec![RepositoryActions::Read, RepositoryActions::Write],
    }
    .insert(&db.pool)
    .await
    .unwrap();
    let auth = token_auth(
        &db.pool,
        user.clone(),
        vec![NRScope::ReadRepository, NRScope::WriteRepository],
    )
    .await;
    assert_eq!(
        npm_request(
            &site,
            "virtual-protected",
            "-/package/demo/dist-tags",
            Method::GET,
            auth.clone()
        )
        .await
        .status(),
        StatusCode::OK
    );
    for method in [Method::PUT, Method::DELETE] {
        assert_eq!(
            npm_request(
                &site,
                "virtual-protected",
                "-/package/demo/dist-tags/beta",
                method,
                auth.clone()
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    NewUserRepositoryPermissions {
        user_id: user.id,
        repository_id: target,
        actions: vec![RepositoryActions::Write],
    }
    .insert(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        npm_request(
            &site,
            "virtual-protected",
            "-/package/demo/dist-tags/beta",
            Method::PUT,
            auth.clone()
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    let Some(DynRepository::NPM(crate::repository::npm::NPMRegistry::Virtual(virtual_repo))) =
        site.get_repository(virtual_id)
    else {
        panic!("virtual repository")
    };
    *virtual_repo.0.publish_to.write() = None;
    assert_eq!(
        npm_request(
            &site,
            "virtual-protected",
            "-/package/demo/dist-tags/beta",
            Method::PUT,
            auth.clone()
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    *virtual_repo.0.publish_to.write() = Some(Uuid::new_v4());
    assert_eq!(
        npm_request(
            &site,
            "virtual-protected",
            "-/package/demo/dist-tags/beta",
            Method::PUT,
            auth
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn npm_default_proxy_indexer_preserves_other_formats_path_policy() {
    use crate::repository::proxy_indexing::{DatabaseProxyIndexer, ProxyIndexing};
    use nr_core::repository::project::ProxyArtifactMeta;
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    let repo: Uuid = sqlx::query_scalar("SELECT id FROM repositories WHERE name='public'")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE repositories SET repository_type='maven' WHERE id=$1")
        .bind(repo)
        .execute(&db.pool)
        .await
        .unwrap();
    DatabaseProxyIndexer::new(site, repo)
        .record_cached_artifact(
            ProxyArtifactMeta::builder("demo", "demo", "Artifacts/Demo-1.0.0.jar")
                .version("1.0.0")
                .build(),
        )
        .await
        .unwrap();
    let path: String =
        sqlx::query_scalar("SELECT path FROM project_versions WHERE repository_id=$1")
            .bind(repo)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(path, "artifacts/demo-1.0.0.jar");
}

#[tokio::test]
async fn npm_invalid_credentials_still_record_the_requested_audit_action() {
    use crate::app::authentication::AuthenticationRaw;
    use tower::ServiceExt;
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    for (method, path, command, action) in [
        (
            Method::GET,
            "-/package/%40scope%2Fdemo/dist-tags",
            "",
            "package.dist_tag.list",
        ),
        (
            Method::PUT,
            "-/package/demo/dist-tags/Beta",
            "",
            "package.dist_tag.set",
        ),
        (
            Method::DELETE,
            "-/package/demo/dist-tags/Beta",
            "",
            "package.dist_tag.delete",
        ),
        (
            Method::PUT,
            "demo",
            "deprecate",
            "package.deprecation.update",
        ),
    ] {
        let ctx = AccessLogContext::default();
        let mut request = http::Request::builder()
            .method(method)
            .uri(format!("/npm-test/protected/{path}"))
            .header("npm-command", command)
            .body(Body::empty())
            .unwrap();
        request.extensions_mut().insert(ctx.clone());
        request
            .extensions_mut()
            .insert(AuthenticationRaw::AuthToken(
                "invalid-npm-test-token".into(),
            ));
        let response = repository_router()
            .with_state(site.clone())
            .oneshot(request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(ctx.snapshot().audit_action.as_deref(), Some(action));
    }
}

#[tokio::test]
async fn shared_directory_lookups_keep_other_formats_case_insensitive() {
    use nr_core::database::entities::project::{
        DBProject, ProjectDBType, versions::DBProjectVersion,
    };
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let _site = npm_site(&db, root.path()).await;
    let repo: Uuid = sqlx::query_scalar("SELECT id FROM repositories WHERE name='public'")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    for format in ["maven", "nuget", "ruby"] {
        sqlx::query("UPDATE repositories SET repository_type=$2 WHERE id=$1")
            .bind(repo)
            .bind(format)
            .execute(&db.pool)
            .await
            .unwrap();
        let id: Uuid = sqlx::query_scalar("INSERT INTO project_versions (project_id,repository_id,version,release_type,path) SELECT id,repository_id,'1.0.0','stable','Org/Demo/1.0.0' FROM projects WHERE repository_id=$1 RETURNING id").bind(repo).fetch_one(&db.pool).await.unwrap();
        for path in ["org/demo/1.0.0", "ORG/DEMO/1.0.0", "Org/Demo/1.0.0"] {
            assert_eq!(
                DBProjectVersion::find_by_version_directory(path, repo, &db.pool)
                    .await
                    .unwrap()
                    .unwrap()
                    .id,
                id,
                "{format}"
            );
            let ids = DBProjectVersion::find_ids_by_version_dir(path, repo, &db.pool)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(ids.version_id, id, "{format}");
            assert_eq!(
                DBProject::find_by_version_directory(path, repo, &db.pool)
                    .await
                    .unwrap()
                    .unwrap()
                    .id,
                ids.project_id,
                "{format}"
            );
        }
        let collision = sqlx::query("INSERT INTO project_versions (project_id,repository_id,version,release_type,path) SELECT id,repository_id,'2.0.0','stable','org/demo/1.0.0' FROM projects WHERE repository_id=$1").bind(repo).execute(&db.pool).await.unwrap_err();
        assert_eq!(
            collision.as_database_error().unwrap().code().as_deref(),
            Some("23505"),
            "{format}"
        );
        sqlx::query("DELETE FROM project_versions WHERE id=$1")
            .bind(id)
            .execute(&db.pool)
            .await
            .unwrap();
    }
}

mod prerelease;
