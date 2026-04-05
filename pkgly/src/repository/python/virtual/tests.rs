#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]

use super::simple_index::{SimpleIndexLink, build_simple_index_html, parse_simple_index_links};

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
