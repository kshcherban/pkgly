//! Grouping module for hosted repository implementations.
//!
//! This module does not introduce new behavior; it re-exports the existing
//! hosted repository types to provide a clearer structure.

pub use crate::repository::cargo::CargoRegistry;
pub use crate::repository::deb::DebRepository;
pub use crate::repository::docker::hosted::DockerHosted;
pub use crate::repository::go::GoRepository;
pub use crate::repository::helm::HelmRepository;
pub use crate::repository::maven::MavenRepository;
pub use crate::repository::npm::NPMRegistry;
pub use crate::repository::php::PhpRepository;
pub use crate::repository::python::PythonRepository;
