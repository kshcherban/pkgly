// ABOUTME: Groups proxy repository modules and their shared utilities.
// ABOUTME: Re-exports format-specific proxy implementations for callers.
//! Grouping module for proxy repository implementations.
//!
//! This module provides a single place to find proxy-related types while
//! keeping existing implementations unchanged.

pub mod base_proxy;

// Re-export the main proxy repository types so callers can opt into
// format-specific proxies from a single place without depending on
// the individual format modules directly.
pub use crate::repository::docker::proxy::DockerProxy;
pub use crate::repository::go::proxy::GoProxy;
pub use crate::repository::maven::proxy::MavenProxy;
pub use crate::repository::npm::proxy::NpmProxyRegistry;
pub use crate::repository::php::proxy::PhpProxy;
pub use crate::repository::python::proxy::PythonProxy;
