use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const VERSIONS: [&str; 3] = ["1.2.3-123", "1.2.3-rc-test.2", "1.2.3-rc.1+Build.7"];
const NAMES: [&str; 2] = ["retrieval-lib", "@scope/retrieval-lib"];

fn bytes(name: &str, version: &str) -> Vec<u8> {
    format!("tarball:{name}:{version}").into_bytes()
}

async fn upstream() -> (String, tokio::task::JoinHandle<()>, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let origin = base.clone();
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let app = axum::Router::new().fallback(move |uri: http::Uri| {
        let origin = origin.clone();
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            let path = uri.path().replace("%2B", "+").replace("%2b", "+");
            let name = if path.contains("scope") {
                NAMES[1]
            } else {
                NAMES[0]
            };
            let version = VERSIONS.iter().find(|v| path.contains(**v)).copied();
            if path.ends_with(".tgz") {
                return (StatusCode::OK, bytes(name, version.unwrap())).into_response();
            }
            let mut versions = serde_json::Map::new();
            for version in VERSIONS {
                let mut manifest =
                    publication("public", name, version, "beta", &bytes(name, version))["versions"]
                        [version]
                        .clone();
                manifest["dist"]["tarball"] =
                    json!(format!("{origin}/{name}/-/retrieval-lib-{version}.tgz"));
                versions.insert(version.into(), manifest);
            }
            let metadata = if let Some(version) = version {
                versions[version].clone()
            } else {
                json!({"name":name,"versions":versions,"dist-tags":{"beta":VERSIONS[2]}})
            };
            axum::Json(metadata).into_response()
        }
    });
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base, task, hits)
}

async fn check_retrieval(
    site: &Pkgly,
    auth: &RepositoryAuthentication,
    repo: &str,
    name: &str,
    version: &str,
) {
    let metadata = json_get(site, auth, repo, name).await;
    let manifest = json_get(site, auth, repo, &format!("{name}/{version}")).await;
    assert_eq!(manifest["version"], version);
    assert_eq!(manifest["name"], name);
    for manifest in [&metadata["versions"][version], &manifest] {
        let url = url::Url::parse(manifest["dist"]["tarball"].as_str().unwrap()).unwrap();
        assert_eq!(
            url.host_str(),
            Some("registry.test"),
            "{repo}: tarball must use the registry"
        );
        let relative = url.path().strip_prefix("/repositories/npm-test/").unwrap();
        let (target, path) = relative.split_once('/').unwrap();
        let response = request(site, auth, target, path, Method::GET, Value::Null).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{repo}: advertised tarball {url}"
        );
        assert_eq!(
            axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .as_ref(),
            bytes(name, version)
        );
    }
    assert_eq!(
        tarball(site, auth, repo, name, version).await,
        bytes(name, version)
    );
}

#[tokio::test]
async fn prerelease_retrieval_preserves_versions_and_bytes_across_repositories_and_cache() {
    let _guard = DB_LOCK.lock().await;
    let (origin, server, hits) = upstream().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site_with_upstream(&db, root.path(), &origin).await;
    let auth = writer(&db).await;
    for name in NAMES {
        for version in VERSIONS {
            assert_eq!(
                request(
                    &site,
                    &auth,
                    "private",
                    name,
                    Method::PUT,
                    publication("private", name, version, "beta", &bytes(name, version))
                )
                .await
                .status(),
                StatusCode::NO_CONTENT
            );
        }
    }
    // Download once through each path, then require proxy reads without the upstream.
    for repo in ["private", "virtual", "proxy-private"] {
        for name in NAMES {
            for version in VERSIONS {
                check_retrieval(&site, &auth, repo, name, version).await;
            }
        }
    }
    let cold_hits = hits.load(Ordering::SeqCst);
    assert!(cold_hits > 0);
    server.abort();
    let _ = server.await;
    for repo in ["private", "virtual", "proxy-private"] {
        for name in NAMES {
            for version in VERSIONS {
                check_retrieval(&site, &auth, repo, name, version).await;
            }
        }
    }
    assert_eq!(hits.load(Ordering::SeqCst), cold_hits);
}

#[tokio::test]
async fn npm_proxy_serves_mixed_metadata_cache_layouts_offline() {
    use crate::repository::Repository;
    use nr_storage::FileContent;

    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    let auth = writer(&db).await;
    let id = sqlx::query_scalar("SELECT id FROM repositories WHERE name='proxy-private'")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let Some(DynRepository::NPM(crate::repository::npm::NPMRegistry::Proxy(proxy))) =
        site.get_repository(id)
    else {
        panic!("expected the npm proxy fixture");
    };
    let storage = proxy.get_storage();
    let version = VERSIONS[2];
    let layouts = [
        ("original", "@scope/original-package", "original-version"),
        ("previous", "@scope/previous-package", "previous-version"),
        ("current", "@scope/current-package", "current-version"),
    ];
    for (layout, package, exact_package) in layouts {
        for (name, exact) in [(package, false), (exact_package, true)] {
            let path = if exact {
                format!("{name}/{version}")
            } else {
                name.to_owned()
            };
            let kind = if exact { "versions" } else { "packages" };
            let current = format!("metadata-v2/{kind}/{path}/document.json");
            let previous = format!("metadata/{path}");
            let manifest = json!({
                "name": name, "version": version,
                "dist": {"tarball": format!("https://upstream.test/{name}/-/package-{version}.tgz")}
            });
            let mut metadata = if exact {
                manifest
            } else {
                json!({"name": name, "versions": {version: manifest}, "dist-tags": {"beta": version}})
            };
            // Keep older copies alongside the current entry so the response proves precedence.
            let entries = match layout {
                "current" => vec![
                    (&path, "stale-original"),
                    (&previous, "stale-previous"),
                    (&current, layout),
                ],
                "previous" => vec![(&previous, layout)],
                _ => vec![(&path, layout)],
            };
            for (cache_path, marker) in entries {
                metadata["description"] = json!(marker);
                storage
                    .save_file(
                        id,
                        FileContent::Content(serde_json::to_vec(&metadata).unwrap()),
                        &StoragePath::from(cache_path.as_str()),
                    )
                    .await
                    .unwrap();
            }
        }
    }
    // The fixture points at a closed port. Every response must come from storage.
    let mut head_failures = Vec::new();
    for (layout, package, exact_package) in layouts {
        for (name, exact) in [(package, false), (exact_package, true)] {
            let path = if exact {
                format!("{name}/{version}")
            } else {
                name.to_owned()
            };
            let metadata = json_get(&site, &auth, "proxy-private", &path).await;
            let head = request(
                &site,
                &auth,
                "proxy-private",
                &path,
                Method::HEAD,
                Value::Null,
            )
            .await;
            if head.status() != StatusCode::OK {
                head_failures.push(format!("{layout} HEAD {path}: {}", head.status()));
            }
            assert_eq!(metadata["description"], layout, "{path}");
            let manifest = if exact {
                &metadata
            } else {
                &metadata["versions"][version]
            };
            assert_eq!(manifest["version"], version, "{path}");
            assert_eq!(
                manifest["dist"]["tarball"],
                format!(
                    "http://registry.test/repositories/npm-test/proxy-private/{name}/-/package-{version}.tgz"
                ),
                "{path}"
            );
        }
    }
    assert!(head_failures.is_empty(), "{}", head_failures.join("\n"));
}
