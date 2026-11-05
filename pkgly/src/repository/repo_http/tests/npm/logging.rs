use super::*;
use std::{
    io,
    sync::{Arc, Mutex},
};
use tracing::instrument::WithSubscriber;
use tracing_subscriber::layer::SubscriberExt;

#[derive(Clone, Default)]
struct Writer(Arc<Mutex<Vec<u8>>>);
impl io::Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("poisoned test log"))?
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Writer {
    type Writer = Self;
    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}

#[tokio::test]
async fn npm_handler_spans_and_audit_events_exclude_request_contents() {
    let _guard = DB_LOCK.lock().await;
    let db = fresh_db().await;
    let root = tempfile::tempdir().unwrap();
    let site = npm_site(&db, root.path()).await;
    let user = npm_user(
        &db.pool,
        "npm-log-user",
        vec![RepositoryActions::Read, RepositoryActions::Write],
    )
    .await;
    sqlx::query("INSERT INTO project_versions (project_id,repository_id,version,release_type,path,extra) SELECT id,repository_id,'1.0.0','stable','demo/1.0.0','{\"extra\":{}}' FROM projects WHERE key='demo'")
        .execute(&db.pool).await.unwrap();
    let writer = Writer::default();
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .json()
            .without_time()
            .with_writer(writer.clone())
            .with_current_span(true)
            .with_span_list(true)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::NEW),
    );
    async {
        for (method, path, command, body) in [
            (Method::GET, "-/package/demo/dist-tags", "", "sensitive-request-body"),
            (Method::PUT, "-/package/demo/dist-tags/beta", "", "\"1.0.0\""),
            (Method::DELETE, "-/package/demo/dist-tags/beta", "", "sensitive-request-body"),
            (Method::PUT, "demo", "deprecate", "{\"name\":\"demo\",\"versions\":{\"1.0.0\":{\"deprecated\":\"sensitive-deprecation-message\"}}}"),
            (Method::PUT, "demo", "publish", r#"{"name":"demo","versions":{"2.0.0":{"name":"demo","version":"2.0.0","dist":{"tarball":"http://registry.test/repositories/npm-test/public/demo/-/demo-2.0.0.tgz","shasum":"hash","integrity":"hash"},"_id":"demo@2.0.0","_nodeVersion":"22","_npmVersion":"11","readme":"sensitive-readme"}},"dist-tags":{"beta":"2.0.0"},"_attachments":{"demo-2.0.0.tgz":{"content_type":"application/octet-stream","data":"ZGVtbw==","length":4}}}"#),
            (Method::GET, "demo", "", "sensitive-request-body"),
        ] {
            let ctx = AccessLogContext::default();
            let mut request = http::Request::builder().method(method.clone()).uri(format!("/repositories/npm-test/public/{path}"))
                .header("npm-command", command).header("Authorization", "Bearer sensitive-credential")
                .body(Body::from(body)).unwrap();
            request.extensions_mut().insert(ctx.clone());
            let response = handle_repo_request_core(site.clone(), RepoRequestPath {storage:"npm-test".into(),repository:"public".into(),path:Some(StoragePath::from(path)),docker_scope:None},
                None, RepositoryAuthentication::Basic(None,user.clone()),request).await.unwrap();
            assert!(response.status().is_success());
            crate::app::request_logging::response_body::emit_audit_log(&tracing::Span::current(),method.as_str(),"/{storage}/{repository}/{*path}",path,Some(i64::from(response.status().as_u16())),&ctx);
        }
        use tower::ServiceExt;
        for raw in [
            crate::app::authentication::AuthenticationRaw::AuthToken("sensitive-credential".into()),
            crate::app::authentication::AuthenticationRaw::AuthorizationHeaderUnknown("unknown".into(), "sensitive-credential".into()),
        ] {
            let mut rejected = http::Request::builder().uri("/npm-test/protected/-/package/demo/dist-tags")
                .body(Body::empty()).unwrap();
            rejected.extensions_mut().insert(raw);
            rejected.extensions_mut().insert(AccessLogContext::default());
            assert_eq!(repository_router().with_state(site.clone()).oneshot(rejected).await.unwrap().status(), StatusCode::UNAUTHORIZED);
        }
    }.with_subscriber(subscriber).await;
    let log = String::from_utf8(writer.0.lock().unwrap().clone()).unwrap();
    for span in [
        "handle_dist_tags",
        "set_dist_tag",
        "handle_deprecate",
        "handle_publish",
    ] {
        assert!(
            log.contains(&format!("\"name\":\"{span}\"")),
            "missing span {span}"
        );
    }
    for action in [
        "package.dist_tag.list",
        "package.dist_tag.set",
        "package.dist_tag.delete",
        "package.deprecation.update",
    ] {
        assert!(log.contains(action), "missing audit action {action}");
    }
    for secret in [
        "sensitive-request-body",
        "sensitive-credential",
        "sensitive-deprecation-message",
        "sensitive-readme",
    ] {
        assert!(
            !log.contains(secret),
            "request contents leaked into tracing"
        );
    }
}
