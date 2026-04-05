use axum::{
    Router,
    extract::{OriginalUri, Path, State},
    response::Response,
    routing::{any, get, post},
};
use http::{StatusCode, header::LOCATION};
use nr_core::database::entities::repository::DBRepositoryWithStorageName;
use tracing::{debug, warn};

use crate::{app::Pkgly, error::InternalError, utils::ResponseBuilder};

pub fn routes() -> Router<Pkgly> {
    Router::new()
        .route("/artifact/{repo}/{*path}", any(artifact_redirect))
        .route("/meta/{repo}/{*group}", get(meta_redirect))
        .route("/{repo}/upload", post(api_upload_redirect))
        .route("/{repo}/{*rest}", any(api_repository_redirect))
        .route("/{repo}", any(api_repository_root_redirect))
}

async fn artifact_redirect(
    State(site): State<Pkgly>,
    Path((repo, tail)): Path<(String, String)>,
    OriginalUri(original): OriginalUri,
) -> Result<Response, InternalError> {
    let repo_info = match resolve_repository(&site, &repo, "artifact request").await? {
        Ok(repo) => repo,
        Err(response) => return Ok(response),
    };
    let location = build_repository_location(
        repo_info.storage_name.as_ref(),
        repo_info.name.as_ref(),
        tail.trim_start_matches('/'),
        original.query(),
    );
    Ok(redirect(location))
}

async fn meta_redirect(
    State(site): State<Pkgly>,
    Path((repo, remainder)): Path<(String, String)>,
    OriginalUri(original): OriginalUri,
) -> Result<Response, InternalError> {
    let repo_info = match resolve_repository(&site, &repo, "metadata request").await? {
        Ok(repo) => repo,
        Err(response) => return Ok(response),
    };
    let trimmed = remainder.trim_start_matches('/');
    let Some(metadata_path) = derive_metadata_path(trimmed) else {
        return Ok(ResponseBuilder::bad_request()
            .body("Invalid metadata request. Expected /api/meta/{repo}/{groupId}/{artifactId}"));
    };
    let location = build_repository_location(
        repo_info.storage_name.as_ref(),
        repo_info.name.as_ref(),
        &metadata_path,
        original.query(),
    );
    Ok(redirect(location))
}

async fn api_repository_redirect(
    State(site): State<Pkgly>,
    Path((repo, rest)): Path<(String, String)>,
    OriginalUri(original): OriginalUri,
) -> Result<Response, InternalError> {
    let repo_info = match resolve_repository(&site, &repo, "API repository route").await? {
        Ok(repo) => repo,
        Err(response) => return Ok(response),
    };
    debug!(?repo, redirected_to = %rest, "Artipie API rewrite with remainder");
    let location = build_repository_location(
        repo_info.storage_name.as_ref(),
        repo_info.name.as_ref(),
        rest.trim_start_matches('/'),
        original.query(),
    );
    Ok(redirect(location))
}

async fn api_upload_redirect(
    State(site): State<Pkgly>,
    Path(repo): Path<String>,
    OriginalUri(original): OriginalUri,
) -> Result<Response, InternalError> {
    let repo_info = match resolve_repository(&site, &repo, "API upload route").await? {
        Ok(repo) => repo,
        Err(response) => return Ok(response),
    };
    debug!(?repo, "Artipie API upload rewrite");
    let location = build_repository_location(
        repo_info.storage_name.as_ref(),
        repo_info.name.as_ref(),
        "upload",
        original.query(),
    );
    Ok(redirect(location))
}

async fn api_repository_root_redirect(
    State(site): State<Pkgly>,
    Path(repo): Path<String>,
    OriginalUri(original): OriginalUri,
) -> Result<Response, InternalError> {
    let repo_info = match resolve_repository(&site, &repo, "API repository route").await? {
        Ok(repo) => repo,
        Err(response) => return Ok(response),
    };
    debug!(?repo, "Artipie API root rewrite");
    let location = build_repository_location(
        repo_info.storage_name.as_ref(),
        repo_info.name.as_ref(),
        "",
        original.query(),
    );
    Ok(redirect(location))
}

fn redirect(location: String) -> Response {
    ResponseBuilder::default()
        .status(StatusCode::TEMPORARY_REDIRECT)
        .header(LOCATION, location)
        .empty()
}

async fn resolve_repository(
    site: &Pkgly,
    repo: &str,
    context: &str,
) -> Result<Result<DBRepositoryWithStorageName, Response>, InternalError> {
    let repositories = DBRepositoryWithStorageName::get_by_name(repo, site.as_ref()).await?;
    match repositories.len() {
        0 => Ok(Err(
            ResponseBuilder::not_found().body(format!("Repository `{repo}` not found"))
        )),
        1 => Ok(Ok(repositories[0].clone())),
        count => {
            warn!(
                ?repo,
                matches = count,
                context,
                "Multiple repositories share the same name"
            );
            Ok(Err(ResponseBuilder::conflict().body(format!(
                "Multiple repositories named `{repo}` found. Specify a unique repository name."
            ))))
        }
    }
}

fn build_repository_location(
    storage: &str,
    repository: &str,
    tail: &str,
    query: Option<&str>,
) -> String {
    let mut path = format!("/repositories/{}/{}", storage, repository);
    let trimmed_tail = tail.trim_start_matches('/');
    if !trimmed_tail.is_empty() {
        if !path.ends_with('/') {
            path.push('/');
        }
        path.push_str(trimmed_tail);
    }
    if let Some(query) = query {
        if !query.is_empty() {
            path.push('?');
            path.push_str(query);
        }
    }
    path
}

fn derive_metadata_path(remainder: &str) -> Option<String> {
    if remainder.is_empty() {
        return None;
    }
    let (group_path, artifact_id) = match remainder.rsplit_once('/') {
        Some(tuple) => tuple,
        None => ("", remainder),
    };
    if artifact_id.is_empty() {
        return None;
    }
    let trimmed_group = group_path.trim_matches('/');
    let mut metadata_path = String::new();
    if !trimmed_group.is_empty() {
        metadata_path.push_str(trimmed_group);
        if !metadata_path.ends_with('/') {
            metadata_path.push('/');
        }
    }
    metadata_path.push_str(artifact_id);
    metadata_path.push_str("/maven-metadata.xml");
    Some(metadata_path)
}

#[cfg(test)]
mod tests;
