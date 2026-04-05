use std::{fmt::Debug, future::Future};

use nr_core::{
    repository::{Visibility, project::ProjectResolution},
    storage::StoragePath,
};
use nr_storage::DynStorage;
use uuid::Uuid;

use crate::{app::Pkgly, utils::IntoErrorResponse};

use super::{RepoResponse, RepositoryFactoryError, RepositoryRequest};

/// Core repository abstraction implemented by all concrete repository types.
pub trait Repository: Send + Sync + Clone + Debug {
    type Error: IntoErrorResponse + 'static;

    fn get_storage(&self) -> DynStorage;

    /// The Repository type. This is used to identify the Repository type in the database.
    fn get_type(&self) -> &'static str;

    fn full_type(&self) -> &'static str {
        self.get_type()
    }

    /// Config types that this Repository type has.
    fn config_types(&self) -> Vec<&str>;

    fn name(&self) -> String;

    fn id(&self) -> Uuid;

    fn visibility(&self) -> Visibility;

    fn is_active(&self) -> bool;

    /// Returns a copy of the site that this Repository is associated with.
    fn site(&self) -> Pkgly;

    fn resolve_project_and_version_for_path(
        &self,
        _path: &StoragePath,
    ) -> impl Future<Output = Result<ProjectResolution, Self::Error>> + Send {
        async { Ok(ProjectResolution::default()) }
    }

    async fn reload(&self) -> Result<(), RepositoryFactoryError> {
        Ok(())
    }

    /// Handles a GET request to a repository.
    fn handle_get(
        &self,
        request: RepositoryRequest,
    ) -> impl Future<Output = Result<RepoResponse, Self::Error>> + Send {
        async {
            Ok(RepoResponse::unsupported_method_response(
                request.parts.method,
                self.get_type(),
            ))
        }
    }

    /// Handles a POST request to a repository.
    fn handle_post(
        &self,
        request: RepositoryRequest,
    ) -> impl Future<Output = Result<RepoResponse, Self::Error>> + Send {
        async {
            Ok(RepoResponse::unsupported_method_response(
                request.parts.method,
                self.get_type(),
            ))
        }
    }

    /// Handles a PUT request to a repository.
    fn handle_put(
        &self,
        request: RepositoryRequest,
    ) -> impl Future<Output = Result<RepoResponse, Self::Error>> + Send {
        async {
            Ok(RepoResponse::unsupported_method_response(
                request.parts.method,
                self.get_type(),
            ))
        }
    }

    /// Handles a PATCH request to a repository.
    fn handle_patch(
        &self,
        request: RepositoryRequest,
    ) -> impl Future<Output = Result<RepoResponse, Self::Error>> + Send {
        async {
            Ok(RepoResponse::unsupported_method_response(
                request.parts.method,
                self.get_type(),
            ))
        }
    }

    /// Handles a DELETE request to a repository.
    fn handle_delete(
        &self,
        request: RepositoryRequest,
    ) -> impl Future<Output = Result<RepoResponse, Self::Error>> + Send {
        async {
            Ok(RepoResponse::unsupported_method_response(
                request.parts.method,
                self.get_type(),
            ))
        }
    }

    /// Handles a HEAD request to a repository.
    fn handle_head(
        &self,
        request: RepositoryRequest,
    ) -> impl Future<Output = Result<RepoResponse, Self::Error>> + Send {
        async {
            Ok(RepoResponse::unsupported_method_response(
                request.parts.method,
                self.get_type(),
            ))
        }
    }

    /// Handles any other HTTP method to a repository.
    fn handle_other(
        &self,
        request: RepositoryRequest,
    ) -> impl Future<Output = Result<RepoResponse, Self::Error>> + Send {
        async {
            Ok(RepoResponse::unsupported_method_response(
                request.parts.method,
                self.get_type(),
            ))
        }
    }
}
