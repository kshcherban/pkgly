use super::*;

async fn hosted(site: &Pkgly) -> DynRepository {
    let id = sqlx::query_scalar("SELECT id FROM repositories WHERE name = 'public'")
        .fetch_one(&site.database)
        .await
        .unwrap();
    site.get_repository(id).unwrap()
}

#[tokio::test]
async fn publish_rejects_unservable_tarballs_before_creating_packages() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    let auth = writer(&db).await;
    let mut failures = Vec::new();
    for (name, version, case) in [
        ("bad-semver", "1.0", "version"),
        ("bad-file", "1.0.0-beta.1", "file"),
        ("bad-url", "1.0.0-beta.1", "url"),
        ("bad-name", "1.0.0-beta.1", "name"),
        ("bad-route", "1.0.0-beta.1", "route"),
    ] {
        let mut body = publication("public", name, version, "beta", BYTES);
        let path = if case == "route" {
            "other-package"
        } else {
            name
        };
        match case {
            "file" => {
                let attachments = body["_attachments"].as_object_mut().unwrap();
                let attachment = attachments
                    .remove(&format!("{name}-{version}.tgz"))
                    .unwrap();
                attachments.insert("bundle.tgz".into(), attachment);
            }
            "url" => {
                body["versions"][version]["dist"]["tarball"] = json!(
                    "http://registry.test/repositories/npm-test/public/another/-/another-1.0.0-beta.1.tgz"
                )
            }
            "name" => body["name"] = json!("other-package"),
            _ => {}
        }
        let status = request(&site, &auth, "public", path, Method::PUT, body)
            .await
            .status();
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE key = $1)")
                .bind(name)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        if status != StatusCode::BAD_REQUEST || exists {
            failures.push(format!("{case}: {status}, project exists: {exists}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn deleting_a_version_clears_its_tags_and_preserves_other_tags() {
    use crate::app::{
        api::repository::packages::delete_cached_package_paths, webhooks::PackageWebhookActor,
    };
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    let auth = writer(&db).await;
    for legacy in [false, true] {
        let name = if legacy {
            "legacy-target"
        } else {
            "tag-target"
        };
        for (version, tag) in [
            ("0.9.0", "next"),
            ("1.0.0", "latest"),
            ("2.0.0-beta.1", "beta"),
        ] {
            assert_eq!(
                request(
                    &site,
                    &auth,
                    "public",
                    name,
                    Method::PUT,
                    publication("public", name, version, tag, BYTES)
                )
                .await
                .status(),
                StatusCode::NO_CONTENT
            );
        }
        if legacy {
            sqlx::query("UPDATE projects SET npm_dist_tags = NULL WHERE key = $1")
                .bind(name)
                .execute(&db.pool)
                .await
                .unwrap();
            assert_eq!(
                json_get(&site, &auth, "public", name).await["dist-tags"],
                json!({"latest":"1.0.0"})
            );
        }
        let repo = hosted(&site).await;
        delete_cached_package_paths(
            &site,
            repo,
            &[format!("{name}/1.0.0/{name}-1.0.0.tgz")],
            PackageWebhookActor {
                user_id: None,
                username: None,
            },
        )
        .await
        .unwrap();
        let metadata = json_get(&site, &auth, "public", name).await;
        assert_eq!(
            metadata["dist-tags"],
            if legacy {
                json!({})
            } else {
                json!({"beta":"2.0.0-beta.1","next":"0.9.0"})
            }
        );
        assert!(metadata["versions"].get("1.0.0").is_none());
        assert_eq!(
            json_get(
                &site,
                &auth,
                "public",
                &format!("-/package/{name}/dist-tags")
            )
            .await,
            metadata["dist-tags"]
        );
    }
}

#[tokio::test]
async fn missing_latest_manifest_uses_newest_readable_metadata_without_moving_the_tag() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    let auth = writer(&db).await;
    let name = "corrupt-latest";
    for (version, tag) in [("1.0.0", "latest"), ("2.0.0-beta.1", "beta")] {
        assert_eq!(
            request(
                &site,
                &auth,
                "public",
                name,
                Method::PUT,
                publication("public", name, version, tag, BYTES)
            )
            .await
            .status(),
            StatusCode::NO_CONTENT
        );
    }
    sqlx::query(
        "UPDATE project_versions SET extra = '{}'::jsonb WHERE path = 'corrupt-latest/1.0.0'",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    let metadata = json_get(&site, &auth, "public", name).await;
    assert_eq!(metadata["readme"], "Original README");
    assert_eq!(metadata["dist-tags"]["latest"], "1.0.0");
}

#[tokio::test]
async fn hosted_tarball_head_returns_metadata_and_no_body() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    let auth = writer(&db).await;
    let name = "head-check";
    let version = "1.0.0-beta.1";
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
    let path = format!("{name}/-/{name}-{version}.tgz");
    let head = request(&site, &auth, "public", &path, Method::HEAD, Value::Null).await;
    assert_eq!(head.status(), StatusCode::OK);
    assert_eq!(
        head.headers()[http::header::CONTENT_LENGTH],
        BYTES.len().to_string()
    );
    assert!(
        axum::body::to_bytes(head.into_body(), usize::MAX)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        request(
            &site,
            &auth,
            "public",
            "head-check/-/head-check-9.0.0.tgz",
            Method::HEAD,
            Value::Null
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
}
