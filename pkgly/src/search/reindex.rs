use anyhow::{anyhow, bail};
use nr_core::database::entities::{
    package_file::DBPackageFile, project::versions::DBProjectVersion,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    app::Pkgly,
    repository::{
        DynRepository, Repository, cargo::CargoRegistry, deb::DebRepository,
        docker::DockerRegistry, go::GoRepository, helm::HelmRepository, maven::MavenRepository,
        npm::NPMRegistry, php::PhpRepository, python::PythonRepository, ruby::RubyRepository,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReindexKind {
    NpmHosted,
    NpmProxy,
    PythonHosted,
    PythonProxy,
    MavenHosted,
    MavenProxy,
    PhpHosted,
    PhpProxy,
    GoHosted,
    GoProxy,
    DockerHosted,
    DockerProxy,
    CargoHosted,
    HelmHosted,
    DebHosted,
    DebProxy,
    RubyHosted,
}

impl ReindexKind {
    fn matches_repository(self, repository: &DynRepository) -> bool {
        matches!(
            (self, repository),
            (
                ReindexKind::NpmHosted,
                DynRepository::NPM(NPMRegistry::Hosted(_))
            ) | (
                ReindexKind::NpmProxy,
                DynRepository::NPM(NPMRegistry::Proxy(_))
            ) | (
                ReindexKind::PythonHosted,
                DynRepository::Python(PythonRepository::Hosted(_))
            ) | (
                ReindexKind::PythonProxy,
                DynRepository::Python(PythonRepository::Proxy(_))
            ) | (
                ReindexKind::MavenHosted,
                DynRepository::Maven(MavenRepository::Hosted(_))
            ) | (
                ReindexKind::MavenProxy,
                DynRepository::Maven(MavenRepository::Proxy(_))
            ) | (
                ReindexKind::PhpHosted,
                DynRepository::Php(PhpRepository::Hosted(_))
            ) | (
                ReindexKind::PhpProxy,
                DynRepository::Php(PhpRepository::Proxy(_))
            ) | (
                ReindexKind::GoHosted,
                DynRepository::Go(GoRepository::Hosted(_))
            ) | (
                ReindexKind::GoProxy,
                DynRepository::Go(GoRepository::Proxy(_))
            ) | (
                ReindexKind::DockerHosted,
                DynRepository::Docker(DockerRegistry::Hosted(_))
            ) | (
                ReindexKind::DockerProxy,
                DynRepository::Docker(DockerRegistry::Proxy(_))
            ) | (
                ReindexKind::CargoHosted,
                DynRepository::Cargo(CargoRegistry::Hosted(_))
            ) | (
                ReindexKind::HelmHosted,
                DynRepository::Helm(HelmRepository::Hosted(_))
            ) | (
                ReindexKind::DebHosted,
                DynRepository::Deb(DebRepository::Hosted(_))
            ) | (
                ReindexKind::DebProxy,
                DynRepository::Deb(DebRepository::Proxy(_))
            ) | (
                ReindexKind::RubyHosted,
                DynRepository::Ruby(RubyRepository::Hosted(_))
            )
        )
    }

    fn as_cli_name(self) -> &'static str {
        match self {
            ReindexKind::NpmHosted => "npm-hosted",
            ReindexKind::NpmProxy => "npm-proxy",
            ReindexKind::PythonHosted => "python-hosted",
            ReindexKind::PythonProxy => "python-proxy",
            ReindexKind::MavenHosted => "maven-hosted",
            ReindexKind::MavenProxy => "maven-proxy",
            ReindexKind::PhpHosted => "php-hosted",
            ReindexKind::PhpProxy => "php-proxy",
            ReindexKind::GoHosted => "go-hosted",
            ReindexKind::GoProxy => "go-proxy",
            ReindexKind::DockerHosted => "docker-hosted",
            ReindexKind::DockerProxy => "docker-proxy",
            ReindexKind::CargoHosted => "cargo-hosted",
            ReindexKind::HelmHosted => "helm-hosted",
            ReindexKind::DebHosted => "deb-hosted",
            ReindexKind::DebProxy => "deb-proxy",
            ReindexKind::RubyHosted => "ruby-hosted",
        }
    }
}

pub async fn reindex_repository(
    site: Pkgly,
    repository_id: Uuid,
    kind: ReindexKind,
) -> anyhow::Result<usize> {
    let repository = site
        .get_repository(repository_id)
        .ok_or_else(|| anyhow!("Repository {repository_id} is not loaded"))?;

    if !kind.matches_repository(&repository) {
        bail!(
            "Repository {repository_id} is of type {} but {} reindexing was requested",
            repository.full_type(),
            kind.as_cli_name()
        );
    }

    reindex_package_file_catalog(&site, repository_id).await
}

async fn reindex_package_file_catalog(site: &Pkgly, repository_id: Uuid) -> anyhow::Result<usize> {
    let rows = sqlx::query_as::<_, DBProjectVersion>(
        r#"
        SELECT *
        FROM project_versions
        WHERE repository_id = $1
        ORDER BY updated_at DESC
        "#,
    )
    .bind(repository_id)
    .fetch_all(&site.database)
    .await?;

    info!(
        repository = %repository_id,
        rows = rows.len(),
        "Reindexing package_files from project_versions"
    );

    let mut processed = 0usize;
    for version in rows.iter() {
        match DBPackageFile::upsert_from_project_version(&site.database, version).await {
            Ok(Some(_)) => {
                processed += 1;
            }
            Ok(None) => {
                warn!(
                    repository = %repository_id,
                    project_id = %version.project_id,
                    version_id = %version.id,
                    "Skipping catalog row with missing project"
                );
            }
            Err(err) => {
                warn!(
                    ?err,
                    repository = %repository_id,
                    project_id = %version.project_id,
                    version_id = %version.id,
                    "Failed to upsert package_files row during reindex"
                );
            }
        }
    }

    Ok(processed)
}

#[cfg(test)]
mod tests {
    use super::ReindexKind;

    #[test]
    fn cli_names_are_stable() {
        assert_eq!(ReindexKind::PythonHosted.as_cli_name(), "python-hosted");
        assert_eq!(ReindexKind::MavenProxy.as_cli_name(), "maven-proxy");
        assert_eq!(ReindexKind::DockerHosted.as_cli_name(), "docker-hosted");
    }
}
