#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
use http_body_util::BodyExt;

#[test]
fn multiple_scope_query_parameters_are_collected() {
    let query: DockerTokenQuery = serde_urlencoded::from_str(
        "service=pkgly&scope=repository%3Arepositories/test/helm/pkgly%3Apull%2Cpush\
        &scope=repository%3Atest/helm/pkgly%3Apull",
    )
    .expect("query should deserialize");

    assert_eq!(query.scope.len(), 2);
    assert!(
        query
            .scope
            .iter()
            .any(|value| value.contains("repositories/test/helm/pkgly"))
    );
    assert!(
        query
            .scope
            .iter()
            .any(|value| value.contains("test/helm/pkgly"))
    );
}

#[test]
fn parse_scopes_handles_repositories_prefix() {
    let scopes = vec!["repository:repositories/test/helm/chart:pull,push".to_string()];
    let parsed = parse_scopes(&scopes).expect("scopes should parse");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].storage, "test");
    assert_eq!(parsed[0].repository, "helm");
    assert!(parsed[0].actions.contains(&RepositoryActions::Read));
    assert!(parsed[0].actions.contains(&RepositoryActions::Write));
}

#[test]
fn authentication_error_sets_www_authenticate_header() {
    let response = DockerTokenError::Authentication.into_response();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let header = response
        .headers()
        .get(http::header::WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok());
    assert_eq!(header, Some("Basic realm=\"Pkgly Docker Token\""));
}

#[tokio::test]
async fn forbidden_error_returns_json_payload() {
    let response = DockerTokenError::Forbidden.into_response();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let collected = response.into_body().collect().await.unwrap();
    let body = collected.to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).expect("body should be valid JSON");
    assert_eq!(
        json,
        serde_json::json!({"errors":[{"code":"DENIED","message":"requested access to the resource is denied"}]})
    );
}
