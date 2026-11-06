use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;

const BYTES: &[u8] = b"original prerelease tarball";

fn publication(repo: &str, name: &str, version: &str, tag: &str, bytes: &[u8]) -> Value {
    let file = format!("{}-{version}.tgz", name.rsplit('/').next().unwrap());
    json!({
        "name":name, "versions":{version:{
            "name":name,"version":version,"_id":format!("{name}@{version}"),
            "_nodeVersion":"22","_npmVersion":"test", "readme":"Original README",
            "dist":{"tarball":format!("http://registry.test/repositories/npm-test/{repo}/{name}/-/{file}"),
                "shasum":"fixture","integrity":"fixture"}
        }},
        "dist-tags":{tag:version},
        "_attachments":{file:{"content_type":"application/octet-stream","length":bytes.len(),"data":STANDARD.encode(bytes)}}
    })
}

async fn request(
    site: &Pkgly,
    auth: &RepositoryAuthentication,
    repo: &str,
    path: &str,
    method: Method,
    body: Value,
) -> Response {
    let request = http::Request::builder()
        .method(method)
        .uri(format!("/repositories/npm-test/{repo}/{path}"))
        .header("host", "registry.test")
        .header("npm-command", "publish")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
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
        auth.clone(),
        request,
    )
    .await
    .unwrap_or_else(IntoResponse::into_response)
}

async fn json_get(site: &Pkgly, auth: &RepositoryAuthentication, repo: &str, path: &str) -> Value {
    let response = request(site, auth, repo, path, Method::GET, Value::Null).await;
    assert_eq!(response.status(), StatusCode::OK, "GET {repo}/{path}");
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap()
}

async fn tarball(
    site: &Pkgly,
    auth: &RepositoryAuthentication,
    repo: &str,
    name: &str,
    version: &str,
) -> Vec<u8> {
    let path = format!(
        "{name}/-/{}-{version}.tgz",
        name.rsplit('/').next().unwrap()
    );
    let response = request(site, auth, repo, &path, Method::GET, Value::Null).await;
    assert_eq!(response.status(), StatusCode::OK, "GET {repo}/{path}");
    axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec()
}

async fn writer(db: &TestDb) -> RepositoryAuthentication {
    RepositoryAuthentication::Basic(
        None,
        npm_user(
            &db.pool,
            "prerelease-writer",
            vec![RepositoryActions::Read, RepositoryActions::Write],
        )
        .await,
    )
}

#[tokio::test]
async fn republishing_prerelease_preserves_version_metadata_and_applies_tags() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    let auth = writer(&db).await;
    let name = "republished-prerelease";
    let version = "1.0.0-beta.1";
    let original = publication("public", name, version, "beta", BYTES);
    assert_eq!(
        request(&site, &auth, "public", name, Method::PUT, original)
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    let before = json_get(&site, &auth, "public", name).await;
    let replacement = b"replacement tarball";
    let mut repeat = publication("public", name, version, "next", replacement);
    repeat["versions"][version]["readme"] = json!("Replacement README");
    assert_eq!(
        request(&site, &auth, "public", name, Method::PUT, repeat)
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    let after = json_get(&site, &auth, "public", name).await;
    assert_eq!(after["versions"], before["versions"]);
    assert_eq!(
        after["dist-tags"],
        json!({"beta": version, "next": version})
    );
    assert_eq!(
        tarball(&site, &auth, "public", name, version).await,
        replacement
    );
}

#[tokio::test]
async fn invalid_prerelease_publications_leave_no_partial_state() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    let auth = writer(&db).await;
    let mut failures = Vec::new();
    for existing in [false, true] {
        for invalid in ["base64", "empty", "version", "tag-name", "tag-target"] {
            let name = format!("invalid-{invalid}-{existing}");
            if existing {
                let body = publication("public", &name, "1.0.0", "latest", BYTES);
                assert_eq!(
                    request(&site, &auth, "public", &name, Method::PUT, body)
                        .await
                        .status(),
                    StatusCode::NO_CONTENT
                );
            }
            let before:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(to_jsonb(v) ORDER BY v.id),'[]') FROM project_versions v JOIN projects p ON p.id=v.project_id WHERE p.key=$1").bind(&name).fetch_one(&db.pool).await.unwrap();
            let before_metadata = if existing {
                Some(json_get(&site, &auth, "public", &name).await)
            } else {
                None
            };
            let mut body = publication("public", &name, "2.0.0-beta.1", "beta", BYTES);
            let file = format!("{name}-2.0.0-beta.1.tgz");
            match invalid {
                "base64" => body["_attachments"][&file]["data"] = json!("not base64!"),
                "empty" => body["_attachments"] = json!({}),
                "version" => body["versions"]["2.0.0-beta.1"]["version"] = json!("2.0.0-beta.2"),
                "tag-name" => body["dist-tags"] = json!({"^1.0.0":"2.0.0-beta.1"}),
                "tag-target" => body["dist-tags"] = json!({"beta":"9.0.0"}),
                _ => unreachable!(),
            }
            let status = request(&site, &auth, "public", &name, Method::PUT, body)
                .await
                .status();
            let after:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(to_jsonb(v) ORDER BY v.id),'[]') FROM project_versions v JOIN projects p ON p.id=v.project_id WHERE p.key=$1").bind(&name).fetch_one(&db.pool).await.unwrap();
            let response = request(&site, &auth, "public", &name, Method::GET, Value::Null).await;
            let metadata_unchanged = if let Some(before) = before_metadata {
                response.status() == StatusCode::OK
                    && serde_json::from_slice::<Value>(
                        &axum::body::to_bytes(response.into_body(), usize::MAX)
                            .await
                            .unwrap(),
                    )
                    .unwrap()
                        == before
            } else {
                response.status() == StatusCode::NOT_FOUND
            };
            let file_status = request(
                &site,
                &auth,
                "public",
                &format!("{name}/-/{file}"),
                Method::GET,
                Value::Null,
            )
            .await
            .status();
            if existing {
                assert_eq!(tarball(&site, &auth, "public", &name, "1.0.0").await, BYTES);
            }
            if status != StatusCode::BAD_REQUEST
                || before != after
                || !metadata_unchanged
                || file_status == StatusCode::OK
            {
                failures.push(format!("{invalid}, existing={existing}: status={status}, catalog unchanged={}, metadata unchanged={metadata_unchanged}, tarball={file_status}",before==after));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn latest_is_protected_but_a_beta_only_package_can_remove_its_final_tag() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    let auth = writer(&db).await;
    for legacy in [false, true] {
        let name = format!("delete-tags-{legacy}");
        assert_eq!(
            request(
                &site,
                &auth,
                "public",
                &name,
                Method::PUT,
                publication("public", &name, "1.0.0", "latest", BYTES)
            )
            .await
            .status(),
            StatusCode::NO_CONTENT
        );
        if legacy {
            sqlx::query("UPDATE projects SET npm_dist_tags=NULL WHERE key=$1")
                .bind(&name)
                .execute(&db.pool)
                .await
                .unwrap();
        }
        let tags = format!("-/package/{name}/dist-tags");
        assert_eq!(
            json_get(&site, &auth, "public", &tags).await,
            json!({"latest":"1.0.0"})
        );
        assert_eq!(
            request(
                &site,
                &auth,
                "public",
                &format!("{tags}/latest"),
                Method::DELETE,
                Value::Null
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            json_get(&site, &auth, "public", &tags).await,
            json!({"latest":"1.0.0"})
        );
        assert_eq!(tarball(&site, &auth, "public", &name, "1.0.0").await, BYTES);
    }
    let name = "beta-only";
    for version in ["1.0.0-beta.1", "1.0.0-beta.2"] {
        assert_eq!(
            request(
                &site,
                &auth,
                "public",
                name,
                Method::PUT,
                publication("public", name, version, "beta", BYTES)
            )
            .await
            .status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            request(
                &site,
                &auth,
                "public",
                "-/package/beta-only/dist-tags/beta",
                Method::DELETE,
                Value::Null
            )
            .await
            .status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            json_get(&site, &auth, "public", name).await["dist-tags"],
            json!({})
        );
    }
}

mod retrieval;
mod review;
