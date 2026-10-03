use regex::Regex;
use std::sync::LazyLock;
use url::Url;

use crate::repository::python::utils::{html_escape, normalize_package_name};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleIndexLink {
    pub href: String,
    pub text: String,
    pub requires_python: Option<String>,
}

static ANCHOR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<a\s+([^>]+)>([^<]*)</a>"#)
        .unwrap_or_else(|e| panic!("invalid anchor regex: {e}"))
});
static HREF_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)\bhref\s*=\s*"([^"]+)""#)
        .unwrap_or_else(|e| panic!("invalid href regex: {e}"))
});
static REQUIRES_PY_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)\bdata-requires-python\s*=\s*"([^"]+)""#)
        .unwrap_or_else(|e| panic!("invalid requires-python regex: {e}"))
});

pub fn parse_simple_index_links(html: &str) -> Vec<SimpleIndexLink> {
    let mut links = Vec::new();
    for caps in ANCHOR_REGEX.captures_iter(html) {
        let attrs = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let text = caps
            .get(2)
            .map(|m| html_unescape_basic(m.as_str().trim()))
            .unwrap_or_default();

        let Some(href) = HREF_REGEX
            .captures(attrs)
            .and_then(|m| m.get(1))
            .map(|m| html_unescape_basic(m.as_str().trim()))
        else {
            continue;
        };

        let requires_python = REQUIRES_PY_REGEX
            .captures(attrs)
            .and_then(|m| m.get(1))
            .map(|m| html_unescape_basic(m.as_str().trim()));

        links.push(SimpleIndexLink {
            href,
            text,
            requires_python,
        });
    }
    links
}

fn html_unescape_basic(input: &str) -> String {
    let mut out = input.replace("&amp;", "&");
    out = out.replace("&lt;", "<");
    out = out.replace("&gt;", ">");
    out = out.replace("&quot;", "\"");
    out = out.replace("&#x27;", "'");
    out = out.replace("&#39;", "'");
    out
}

pub fn build_simple_index_html(
    display_name: &str,
    canonical_repository_base: &str,
    member_links: Vec<(u32, String, Vec<SimpleIndexLink>)>,
) -> String {
    let normalized_package = normalize_package_name(display_name);
    let mut rows = Vec::new();
    for (priority, member_name, links) in member_links {
        for link in links {
            let link = SimpleIndexLink {
                href: canonicalize_member_link(
                    &link.href,
                    canonical_repository_base,
                    &normalized_package,
                ),
                ..link
            };
            rows.push((priority, member_name.clone(), link));
        }
    }
    rows.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.text.cmp(&b.2.text))
    });

    let mut seen = std::collections::HashSet::new();
    let mut merged = Vec::new();
    for (_, _, link) in rows {
        let key = dedup_key(&link.href);
        if !seen.insert(key) {
            continue;
        }
        merged.push(link);
    }

    let mut body = format!(
        "<!DOCTYPE html>\n<html>\n  <head>\n    <meta charset=\"utf-8\">\n    <title>Links for {}</title>\n  </head>\n  <body>\n    <h1>Links for {}</h1>\n",
        html_escape(display_name),
        html_escape(display_name)
    );

    if merged.is_empty() {
        body.push_str("    <p>No files available.</p>\n");
    } else {
        for link in merged {
            body.push_str("    <a href=\"");
            body.push_str(&html_escape(&link.href));
            body.push('"');
            if let Some(rp) = link.requires_python.as_deref() {
                body.push_str(" data-requires-python=\"");
                body.push_str(&html_escape(rp));
                body.push('"');
            }
            body.push('>');
            body.push_str(&html_escape(&link.text));
            body.push_str("</a><br/>\n");
        }
    }

    body.push_str("  </body>\n</html>\n");
    body
}

fn dedup_key(href: &str) -> String {
    href.split('#').next().unwrap_or(href).to_string()
}

fn canonicalize_member_link(
    href: &str,
    canonical_repository_base: &str,
    normalized_package: &str,
) -> String {
    if is_absolute_link(href) {
        return href.to_string();
    }

    let canonical_prefix = format!("{canonical_repository_base}/");
    if href.starts_with('/') {
        if href.starts_with(&canonical_prefix) {
            return href.to_string();
        }
        return format!("{canonical_repository_base}/{}", &href[1..]);
    }

    // Resolve relative links with a placeholder origin. Return only the path, query, and fragment.
    let package_index_url =
        format!("https://pkgly.invalid{canonical_repository_base}/simple/{normalized_package}/");
    let Ok(package_index_url) = Url::parse(&package_index_url) else {
        return href.to_string();
    };
    let Ok(artifact_url) = package_index_url.join(href) else {
        return href.to_string();
    };
    root_relative_href(&artifact_url)
}

fn is_absolute_link(href: &str) -> bool {
    Url::parse(href).is_ok_and(|url| !url.scheme().is_empty()) || href.starts_with("//")
}

fn root_relative_href(url: &Url) -> String {
    let mut href = url.path().to_string();
    if let Some(query) = url.query() {
        href.push('?');
        href.push_str(query);
    }
    if let Some(fragment) = url.fragment() {
        href.push('#');
        href.push_str(fragment);
    }
    href
}
