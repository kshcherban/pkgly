#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use url::Url;

#[test]
fn cache_path_for_simple_package() {
    let path = StoragePath::from("simple/Example_Pkg/example-1.0.0.whl");
    let cache = cache_path_for_python_proxy(&path).expect("cache path");
    assert_eq!(cache.to_string(), "packages/example-pkg/example-1.0.0.whl");
}

#[test]
fn cache_path_for_directory_returns_none() {
    let path = StoragePath::from("simple/example/");
    assert!(cache_path_for_python_proxy(&path).is_none());
}

#[test]
fn cache_path_preserves_existing_packages_path() {
    let path = StoragePath::from("packages/example/example-1.0.0.whl");
    let cache = cache_path_for_python_proxy(&path).expect("cache path");
    assert_eq!(cache.to_string(), "packages/example/example-1.0.0.whl");
}

#[test]
fn normalize_routes_injects_default_when_empty() {
    let routes = normalize_routes(Vec::new());
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0], DEFAULT_ROUTE.clone());
}

#[test]
fn normalize_routes_retains_existing_entries() {
    let custom = PythonProxyRoute {
        url: ProxyURL::try_from(String::from("https://internal.example/simple"))
            .expect("valid url"),
        name: Some("Internal".to_string()),
    };
    let routes = normalize_routes(vec![custom.clone()]);
    assert_eq!(routes, vec![custom]);
}

#[test]
fn request_repository_base_uses_virtual_context_independent_of_uri() {
    for repository in ["test-storage/python-virtual", "TEST-STORAGE/Python-Virtual"] {
        for package in ["demo", "demo/", "d%65mo", "d%65mo/"] {
            for route_prefix in ["/repositories", "/storages", ""] {
                let uri = format!("{route_prefix}/{repository}/simple/{package}");
                let (mut parts, ()) = http::Request::get(&uri)
                    .body(())
                    .expect("request")
                    .into_parts();
                parts.extensions.insert(VirtualRepositoryBasePath(
                    "/repositories/test-storage/python-virtual".to_string(),
                ));

                assert_eq!(
                    request_repository_base(
                        &parts,
                        "/repositories/test-storage/python-proxy".to_string()
                    ),
                    "/repositories/test-storage/python-virtual",
                    "virtual repository base for {uri}"
                );
            }
        }
    }
}

#[test]
fn request_repository_base_uses_proxy_identity_without_virtual_context() {
    for repository in ["test-storage/python-proxy", "TEST-STORAGE/Python-Proxy"] {
        for package in ["demo", "demo/", "d%65mo", "d%65mo/"] {
            for route_prefix in ["/repositories", "/storages", ""] {
                let uri = format!("{route_prefix}/{repository}/simple/{package}");
                let (parts, ()) = http::Request::get(&uri)
                    .body(())
                    .expect("request")
                    .into_parts();

                assert_eq!(
                    request_repository_base(
                        &parts,
                        "/repositories/test-storage/python-proxy".to_string()
                    ),
                    "/repositories/test-storage/python-proxy",
                    "proxy repository base for {uri}"
                );
            }
        }
    }
}

#[test]
fn rewrite_simple_html_uses_virtual_repository_base() {
    let upstream = Url::parse("https://files.pythonhosted.org/packages/demo/demo-1.0.whl")
        .expect("upstream url");

    let html = rewrite_simple_html(
        br#"<a href="https://files.pythonhosted.org/packages/demo/demo-1.0.whl?download=1#sha256=abc">demo</a>"#,
        "/repositories/test-storage/python-virtual",
        &upstream,
    )
    .expect("valid html");

    assert_eq!(
        std::str::from_utf8(&html).expect("rewritten html"),
        r#"<a href="/repositories/test-storage/python-virtual/packages/demo/demo-1.0.whl?download=1#sha256=abc">demo</a>"#
    );
}

#[test]
fn rewrite_simple_html_uses_proxy_repository_base() {
    let upstream = Url::parse("https://files.pythonhosted.org/packages/demo/demo-1.0.whl")
        .expect("upstream url");

    let html = rewrite_simple_html(
        br#"<a href="https://files.pythonhosted.org/packages/demo/demo-1.0.whl?download=1#sha256=def">demo</a>"#,
        "/repositories/test-storage/python-proxy",
        &upstream,
    )
    .expect("valid html");

    assert_eq!(
        std::str::from_utf8(&html).expect("rewritten html"),
        r#"<a href="/repositories/test-storage/python-proxy/packages/demo/demo-1.0.whl?download=1#sha256=def">demo</a>"#
    );
}

#[test]
fn rewrite_simple_html_handles_encoded_and_plain_upstream_link_forms() {
    let upstream = Url::parse("https://pypi.org/simple/demo/").expect("upstream url");

    for repository_base in [
        "/repositories/test-storage/python-proxy",
        "/repositories/test-storage/python-virtual",
    ] {
        for artifact in [
            "demo.whl?download=1#sha256=abc",
            "d%65mo.whl?download=a%2Fb#sha256=abc",
        ] {
            for href in [
                format!("../../packages/{artifact}"),
                format!("/packages/{artifact}"),
                format!("https://pypi.org/packages/{artifact}"),
            ] {
                let source = format!("<a href=\"{href}\">demo</a>");
                let rewritten = rewrite_simple_html(source.as_bytes(), repository_base, &upstream)
                    .expect("valid html");

                assert_eq!(
                    std::str::from_utf8(&rewritten).expect("rewritten html"),
                    format!("<a href=\"{repository_base}/packages/{artifact}\">demo</a>"),
                    "upstream link {href} with repository base {repository_base}"
                );
            }
        }
    }
}

fn metadata_path() -> StoragePath {
    StoragePath::from(
        "packages/bc/66/875d449b23194f45debb8a2b70c704217f0aa2700d967098b2e1b812dd44/parallel_ssh-2.12.0-py3-none-any.whl",
    )
}

#[test]
fn python_proxy_meta_from_cache_path_extracts_wheel_fields() {
    let path = metadata_path();
    let url = Url::parse("https://pypi.org/packages/example.whl").expect("url");
    let meta = super::python_proxy_meta_from_cache_path(&path, 2048, Some(&url)).expect("metadata");
    assert_eq!(meta.package_name, "parallel_ssh");
    assert_eq!(meta.package_key, "parallel-ssh");
    assert_eq!(meta.version.as_deref(), Some("2.12.0"));
    assert_eq!(meta.cache_path, path.to_string());
    assert_eq!(meta.size, Some(2048));
    assert_eq!(
        meta.upstream_url.as_deref(),
        Some("https://pypi.org/packages/example.whl")
    );
}

#[test]
fn python_proxy_meta_from_cache_path_handles_sdist_suffix() {
    let path = StoragePath::from("packages/ab/cd/ef/pkg-name-0.9.0.tar.gz");
    let meta = super::python_proxy_meta_from_cache_path(&path, 512, None).expect("metadata");
    assert_eq!(meta.package_name, "pkg-name");
    assert_eq!(meta.package_key, "pkg-name");
    assert_eq!(meta.version.as_deref(), Some("0.9.0"));
    assert_eq!(meta.cache_path, path.to_string());
    assert_eq!(meta.size, Some(512));
}

#[test]
fn python_proxy_meta_from_cache_path_accepts_v_prefixed_versions() {
    let path = StoragePath::from("packages/example/example-v1.0.0.whl");
    let meta = super::python_proxy_meta_from_cache_path(&path, 256, None).expect("metadata");
    assert_eq!(meta.version.as_deref(), Some("1.0.0"));
}

#[test]
fn python_proxy_key_from_cache_path_extracts_wheel_fields() {
    let path = metadata_path();
    let key = super::python_proxy_key_from_cache_path(&path).expect("key");
    assert_eq!(key.package_key, "parallel-ssh");
    assert_eq!(key.version.as_deref(), Some("2.12.0"));
    assert_eq!(key.cache_path, Some(path.to_string()));
}
