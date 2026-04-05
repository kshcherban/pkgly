//! Common proxy utilities and traits for proxy repositories.
//!
//! Concrete proxy implementations (Docker, Go, Maven, NPM, Python, etc.)
//! live in their respective format-specific modules. This module provides
//! a shared marker trait and shared helpers so higher-level code can reason
//! about "proxy" repositories as a group without depending on each format.
//!
//! The `record_proxy_cache_hit` and `evict_proxy_cache_entry` helpers centralize
//! the common “if Some(meta/key) then index/evict” pattern used by all proxy
//! repositories so individual formats only need to focus on converting storage
//! paths into `ProxyArtifactMeta` / `ProxyArtifactKey`.

use nr_core::repository::project::{ProxyArtifactKey, ProxyArtifactMeta};

use crate::repository::proxy_indexing::{ProxyIndexing, ProxyIndexingError};

/// Marker trait for proxy repositories.
///
/// Implemented by format-specific proxy repository types such as:
/// - `go::proxy::GoProxy`
/// - `maven::proxy::MavenProxy`
/// - `npm::proxy::NpmProxyRegistry`
/// - `python::proxy::PythonProxy`
/// - `docker::proxy::DockerProxy`
pub trait ProxyRepository {}

/// Record a cached proxy artifact if metadata is available.
///
/// This helper encapsulates the common pattern:
/// - Attempt to derive `ProxyArtifactMeta` for a given storage path.
/// - Call `ProxyIndexing::record_cached_artifact` only when metadata exists.
pub async fn record_proxy_cache_hit(
    indexer: &dyn ProxyIndexing,
    meta: Option<ProxyArtifactMeta>,
) -> Result<(), ProxyIndexingError> {
    if let Some(meta) = meta {
        indexer.record_cached_artifact(meta).await?;
    }
    Ok(())
}

/// Evict a cached proxy artifact from the catalog when a key is available.
///
/// This mirrors [`record_proxy_cache_hit`] but for eviction: callers convert
/// storage paths into a `ProxyArtifactKey` and pass it here; if key derivation
/// fails, the indexer is not invoked.
pub async fn evict_proxy_cache_entry(
    indexer: &dyn ProxyIndexing,
    key: Option<ProxyArtifactKey>,
) -> Result<(), ProxyIndexingError> {
    if let Some(key) = key {
        indexer.evict_cached_artifact(key).await?;
    }
    Ok(())
}

// Implement the marker trait for the known proxy repository types. This keeps
// behavior unchanged while giving the type system a way to talk about "any
// proxy repository" when needed.
impl ProxyRepository for crate::repository::go::proxy::GoProxy {}
impl ProxyRepository for crate::repository::maven::proxy::MavenProxy {}
impl ProxyRepository for crate::repository::npm::proxy::NpmProxyRegistry {}
impl ProxyRepository for crate::repository::python::proxy::PythonProxy {}
impl ProxyRepository for crate::repository::docker::proxy::DockerProxy {}
impl ProxyRepository for crate::repository::php::proxy::PhpProxy {}

#[cfg(test)]
mod tests;
