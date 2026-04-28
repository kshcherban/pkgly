use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryRef {
    Id(Uuid),
    Names { storage: String, repository: String },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RepositoryRefError {
    #[error("repository ref must be a UUID or storage/repository")]
    Invalid,
}

impl RepositoryRef {
    pub fn parse(value: &str) -> Result<Self, RepositoryRefError> {
        if let Ok(id) = Uuid::parse_str(value) {
            return Ok(Self::Id(id));
        }
        let Some((storage, repository)) = value.split_once('/') else {
            return Err(RepositoryRefError::Invalid);
        };
        if storage.trim().is_empty() || repository.trim().is_empty() || repository.contains('/') {
            return Err(RepositoryRefError::Invalid);
        }
        Ok(Self::Names {
            storage: storage.to_string(),
            repository: repository.to_string(),
        })
    }
}

impl From<RepositoryRefError> for crate::CliError {
    fn from(value: RepositoryRefError) -> Self {
        Self::Message(value.to_string())
    }
}
