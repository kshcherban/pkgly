use thiserror::Error;

use crate::{cli::NativeCommands, config::ResolvedConfig, repo_ref::RepositoryRef};

#[derive(Debug, Error)]
pub enum NativeError {
    #[error("native commands require repository refs in storage/repository form")]
    RepositoryNamesRequired,
}

pub fn render(command: NativeCommands, config: &ResolvedConfig) -> Result<String, NativeError> {
    match command {
        NativeCommands::Npm { repository } => {
            let (storage, repo) = names(&repository)?;
            let base = config.base_url.trim_end_matches('/');
            Ok(format!(
                "npm config set registry {base}/repositories/{storage}/{repo}/ && npm login --registry {base}/repositories/{storage}/{repo}/ && npm publish --registry {base}/repositories/{storage}/{repo}/"
            ))
        }
        NativeCommands::Cargo { repository } => {
            let (storage, repo) = names(&repository)?;
            let base = config.base_url.trim_end_matches('/');
            Ok(format!(
                "cargo publish --registry pkgly --index {base}/repositories/{storage}/{repo}/api/v1/crates"
            ))
        }
        NativeCommands::Docker { repository, image } => {
            let (storage, repo) = names(&repository)?;
            let base = config
                .base_url
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_end_matches('/');
            let image = image.unwrap_or_else(|| "IMAGE[:TAG]".to_string());
            Ok(format!(
                "docker login {base} && docker tag {image} {base}/repositories/{storage}/{repo}/{image} && docker push {base}/repositories/{storage}/{repo}/{image}"
            ))
        }
    }
}

fn names(value: &str) -> Result<(String, String), NativeError> {
    match RepositoryRef::parse(value).map_err(|_| NativeError::RepositoryNamesRequired)? {
        RepositoryRef::Names {
            storage,
            repository,
        } => Ok((storage, repository)),
        RepositoryRef::Id(_) => Err(NativeError::RepositoryNamesRequired),
    }
}

impl From<NativeError> for crate::CliError {
    fn from(value: NativeError) -> Self {
        Self::Message(value.to_string())
    }
}
