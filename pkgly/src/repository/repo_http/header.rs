use http::HeaderName;
use tracing::{debug, instrument, trace, warn};

use super::RepositoryRequest;
use crate::{
    repository::RepositoryHandlerError,
    utils::{bad_request::BadRequestErrors, header::HeaderValueExt},
};
/// Pkgly Deploy is a custom header used to identify that the request is coming from a Pkgly Deploy Client
pub const PKGLY_REPO_DEPLOY_HEADER: HeaderName = HeaderName::from_static("x-pkgly-deploy");
/// Header Structure for Pkgly Deploy
///
/// The value should be formatted as follows: `{Repository Type} {Version}`
///
/// Not all repositories will have a custom deploy system.
#[derive(Debug)]
pub struct PkglyRepoDeployHeaderValue {
    pub repository_type: String,
    pub version: u8,
}
impl TryFrom<String> for PkglyRepoDeployHeaderValue {
    type Error = BadRequestErrors;
    #[instrument(name = "PkglyRepoDeployHeaderValue::try_from")]
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let params: Vec<_> = value.trim().split(" ").collect();
        if params.len() != 2 {
            warn!(?value, "Invalid Pkgly Deploy Header Value");
            return Err(BadRequestErrors::Other(format!(
                "Invalid Pkgly Deploy Header Value: {}",
                value
            )));
        }
        let repository_type = params[0].to_owned();
        let version: u8 = params[1].parse().map_err(|err| {
            warn!(?err, "Invalid Pkgly Deploy Header Value");
            BadRequestErrors::Other(format!("Invalid Pkgly Deploy Header Value: {}", value))
        })?;
        Ok(Self {
            repository_type,
            version,
        })
    }
}
impl RepositoryRequest {
    #[inline(always)]
    pub fn headers(&self) -> &http::HeaderMap {
        &self.parts.headers
    }
    #[instrument(skip(self))]
    pub fn get_pkgly_deploy_header(
        &self,
    ) -> Result<Option<PkglyRepoDeployHeaderValue>, RepositoryHandlerError> {
        let Some(header) = self.headers().get(PKGLY_REPO_DEPLOY_HEADER) else {
            debug!("No Pkgly Deploy Header Found");
            return Ok(None);
        };
        trace!(?header, "Found Pkgly Deploy Header");
        let header = header.to_string().map_err(BadRequestErrors::from)?;
        debug!(?header, "Header Parsed to String");
        let value = PkglyRepoDeployHeaderValue::try_from(header)?;
        Ok(Some(value))
    }
}
