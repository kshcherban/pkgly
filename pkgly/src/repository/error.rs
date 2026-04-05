//! Backwards-compatible re-exports of repository error types.
//!
//! The concrete definitions now live under `crate::error::repository` so that
//! all top-level error types are centralized, but the public API surface of
//! `crate::repository` remains unchanged.

pub use crate::error::repository::{DynRepositoryHandlerError, RepositoryHandlerError};
