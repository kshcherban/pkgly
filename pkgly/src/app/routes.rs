use axum::{Router, extract::DefaultBodyLimit};
use http::{HeaderName, HeaderValue};
use tower_http::set_header::SetResponseHeaderLayer;
use tracing::info;

use crate::app::{
    api, authentication::layer::AuthenticationLayer, config::MaxUpload, frontend, open_api,
    request_logging::AppTracingLayer,
};

use super::Pkgly;

const POWERED_BY_HEADER: HeaderName = HeaderName::from_static("x-powered-by");
const POWERED_BY_VALUE: HeaderValue = HeaderValue::from_static("Pkgly");

/// Build the main Axum application router for Pkgly.
pub fn build_app_router(site: Pkgly, max_upload: MaxUpload, open_api_routes: bool) -> Router {
    let auth_layer = AuthenticationLayer::from(site.clone());

    // Docker V2 API compatibility routes added directly (not nested) to handle trailing slashes properly
    info!("Docker V2 routes will be added at /v2");

    let mut app = Router::new()
        // Docker Registry V2 API compatibility route
        // Handle both /v2 and /v2/ explicitly to work around Axum nesting trailing slash behavior
        .route(
            "/v2",
            axum::routing::any(crate::repository::handle_docker_v2_base_public),
        )
        .route(
            "/v2/",
            axum::routing::any(crate::repository::handle_docker_v2_base_public),
        )
        .route(
            "/v2/token",
            axum::routing::get(crate::repository::docker::auth::handle_docker_token),
        )
        // Nest the full Docker router for all other V2 paths
        .route(
            "/v2/{*path}",
            axum::routing::any(crate::repository::handle_docker_v2_any_path),
        )
        .nest("/repositories", crate::repository::repository_router())
        .nest("/storages", crate::repository::repository_router())
        // Serve the SPA root explicitly before falling back for other routes
        .route("/", axum::routing::any(frontend::frontend_request))
        .nest("/api", api::api_routes())
        // Direct repository routes for patterns like /{storage}/{repository}/{*path}
        .route(
            "/{storage}/{repository}/{*path}",
            axum::routing::any(crate::repository::handle_repo_request),
        )
        .fallback(frontend::frontend_request)
        .with_state(site.clone());

    if open_api_routes {
        info!("OpenAPI routes enabled");
        app = app.merge(open_api::build_router())
    }

    let body_limit: DefaultBodyLimit = max_upload.into();

    app.layer(auth_layer)
        .layer(SetResponseHeaderLayer::if_not_present(
            POWERED_BY_HEADER,
            POWERED_BY_VALUE,
        ))
        .layer(AppTracingLayer(site))
        .layer(body_limit)
}
