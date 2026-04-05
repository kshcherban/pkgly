//! Authentication provider helpers (OAuth2 / OIDC, JWKS).
//!
//! This module groups the main provider-facing authentication services
//! so call sites have a single, discoverable entry point.
//! The concrete implementations remain in `oauth` and `jwks` modules.

pub use super::jwks::{
    JwkDocument, JwkKey, JwksError, JwksFetcher, JwksManager, JwksResolver, ReqwestJwksFetcher,
};
pub use super::oauth::{OAuth2Rbac, OAuth2Service, OAuth2ServiceError};
