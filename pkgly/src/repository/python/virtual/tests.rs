#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]

use super::simple_index::{SimpleIndexLink, build_simple_index_html, parse_simple_index_links};
use super::{SimpleRequest, StoragePath, redirect_to_trailing_slash};
use http::header::LOCATION;

#[test]
fn parse_simple_index_links_extracts_href_and_requires_python() {
    let html = r#"
      <html><body>
        <a href="pkg-1.0.0.whl" data-requires-python="&gt;=3.8">pkg-1.0.0.whl</a><br/>
        <a href="pkg-1.0.0.tar.gz">pkg-1.0.0.tar.gz</a>
      </body></html>
    "#;

    let links = parse_simple_index_links(html);
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].href, "pkg-1.0.0.whl");
    assert_eq!(links[0].text, "pkg-1.0.0.whl");
    assert_eq!(links[0].requires_python.as_deref(), Some(">=3.8"));
    assert_eq!(links[1].href, "pkg-1.0.0.tar.gz");
}

#[test]
fn redirect_for_nested_stripped_uri_uses_canonical_repository_prefix() {
    let repository_relative_path = StoragePath::from("simple/pkg");
    let request = SimpleRequest::try_from_request(
        &repository_relative_path,
        "/test-storage/python-virtual/simple/pkg",
    )
    .expect("simple package request");
    assert!(request.redirect_needed);

    let response = redirect_to_trailing_slash(
        "/repositories/test-storage/python-virtual",
        &repository_relative_path,
    );

    assert_eq!(response.status(), http::StatusCode::MOVED_PERMANENTLY);
    assert_eq!(
        response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/repositories/test-storage/python-virtual/simple/pkg/")
    );
}

#[test]
fn build_simple_index_html_unions_and_deduplicates_by_href_in_priority_order() {
    let a_links = vec![
        SimpleIndexLink {
            href: "a.whl#sha256=aaa".to_string(),
            text: "a.whl".to_string(),
            requires_python: None,
        },
        SimpleIndexLink {
            href: "shared.whl#sha256=111".to_string(),
            text: "shared.whl".to_string(),
            requires_python: Some(">=3.10".to_string()),
        },
    ];
    let b_links = vec![
        SimpleIndexLink {
            href: "shared.whl#sha256=222".to_string(),
            text: "shared.whl".to_string(),
            requires_python: Some(">=3.11".to_string()),
        },
        SimpleIndexLink {
            href: "b.whl#sha256=bbb".to_string(),
            text: "b.whl".to_string(),
            requires_python: None,
        },
    ];

    let html = build_simple_index_html(
        "Pkg",
        "/repositories/test-storage/python-virtual",
        vec![
            (0, "a".to_string(), a_links),
            (10, "b".to_string(), b_links),
        ],
    );

    let first_shared = html.find("shared.whl#sha256=111").expect("shared link");
    let second_shared = html.find("shared.whl#sha256=222");
    assert!(
        second_shared.is_none(),
        "expected duplicate href to be dropped in favor of the higher priority member"
    );

    let a_pos = html.find("a.whl#sha256=aaa").expect("a link");
    let b_pos = html.find("b.whl#sha256=bbb").expect("b link");
    assert!(a_pos < first_shared && first_shared < b_pos);
    assert!(html.contains("data-requires-python=\"&gt;=3.10\""));
}

#[test]
fn build_simple_index_html_canonicalizes_hosted_member_relative_href() {
    let hosted_html = r#"
        <a href="../../demo-pkg/1.0.0/demo_pkg-1.0.0-py3-none-any.whl?download=1#sha256=3a96">pkgly wheel</a>
        <a href="https://files.pythonhosted.org/packages/pkgly.whl?download=1#sha256=external">external wheel</a>
    "#;
    let hosted_links = parse_simple_index_links(hosted_html);

    let html = build_simple_index_html(
        "demo-pkg",
        "/repositories/test-storage/python-virtual",
        vec![(0, "python-hosted".to_string(), hosted_links)],
    );
    let rendered_links = parse_simple_index_links(&html);

    let hosted_link = rendered_links
        .iter()
        .find(|link| link.text == "pkgly wheel")
        .expect("hosted wheel");
    assert_eq!(
        hosted_link.href,
        "/repositories/test-storage/python-virtual/demo-pkg/1.0.0/demo_pkg-1.0.0-py3-none-any.whl?download=1#sha256=3a96"
    );
    let external_link = rendered_links
        .iter()
        .find(|link| link.text == "external wheel")
        .expect("external wheel");
    assert_eq!(
        external_link.href,
        "https://files.pythonhosted.org/packages/pkgly.whl?download=1#sha256=external"
    );
}

#[test]
fn build_simple_index_html_preserves_href_when_repository_name_case_differs() {
    // Repository lookup accepts these names with different casing from the canonical base below.
    let href = "/repositories/Test-Storage/Python-Virtual/packages/demo.whl?download=1#sha256=abc";
    let html = build_simple_index_html(
        "demo",
        "/repositories/test-storage/python-virtual",
        vec![(
            0,
            "python-proxy".to_string(),
            vec![SimpleIndexLink {
                href: href.to_string(),
                text: "demo.whl".to_string(),
                requires_python: None,
            }],
        )],
    );

    assert_eq!(
        parse_simple_index_links(&html)[0].href,
        href,
        "Different repository-name casing must not duplicate the repository prefix"
    );
}

#[test]
fn build_simple_index_html_preserves_other_repository_href() {
    let href = "/repositories/test-storage/python-proxy/packages/demo.whl?download=1#sha256=abc";
    let html = build_simple_index_html(
        "demo",
        "/repositories/test-storage/python-virtual",
        vec![(
            0,
            "python-proxy".to_string(),
            vec![SimpleIndexLink {
                href: href.to_string(),
                text: "demo.whl".to_string(),
                requires_python: None,
            }],
        )],
    );

    assert_eq!(parse_simple_index_links(&html)[0].href, href);
}

#[test]
fn build_simple_index_html_preserves_encoded_and_plain_root_relative_and_absolute_links() {
    for artifact in [
        "demo.whl?download=1#sha256=abc",
        "d%65mo.whl?download=a%2Fb#sha256=abc",
    ] {
        for prefix in [
            "/repositories/test-storage/python-virtual/packages/",
            "/repositories/Test-Storage/Python-Virtual/packages/",
            "/repositories/Test-Storage/Python-Proxy/packages/",
            "https://files.pythonhosted.org/packages/",
            "//files.pythonhosted.org/packages/",
        ] {
            let href = format!("{prefix}{artifact}");
            let member_html = format!("<a href=\"{href}\">demo</a>");
            let links = parse_simple_index_links(&member_html);
            let html = build_simple_index_html(
                "demo",
                "/repositories/test-storage/python-virtual",
                vec![(0, "python-proxy".to_string(), links)],
            );

            assert_eq!(
                parse_simple_index_links(&html)[0].href,
                href,
                "member link {href}"
            );
        }
    }
}

#[test]
fn build_simple_index_html_resolves_encoded_and_plain_relative_links() {
    for artifact in [
        "demo.whl?download=1#sha256=abc",
        "d%65mo.whl?download=a%2Fb#sha256=abc",
    ] {
        let member_html = format!("<a href=\"../../packages/{artifact}\">demo</a>");
        let links = parse_simple_index_links(&member_html);
        let html = build_simple_index_html(
            "demo",
            "/repositories/test-storage/python-virtual",
            vec![(0, "python-hosted".to_string(), links)],
        );

        assert_eq!(
            parse_simple_index_links(&html)[0].href,
            format!("/repositories/test-storage/python-virtual/packages/{artifact}"),
            "relative link for {artifact}"
        );
    }
}
