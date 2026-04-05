#![allow(
    elided_lifetimes_in_paths,
    clippy::all,
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::clone_on_copy,
    clippy::redundant_closure,
    clippy::needless_return,
    clippy::redundant_pattern_matching,
    clippy::type_complexity,
    clippy::unnecessary_lazy_evaluations,
    clippy::manual_pattern_char_comparison,
    clippy::useless_conversion,
    clippy::map_clone,
    clippy::too_many_arguments,
    clippy::disallowed_types,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::todo
)]
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::config::{PkglyConfig, load_config};
use anyhow::Context;
use app::Pkgly;
use app::web::resolve_worker_threads;
use clap::{Parser, Subcommand};
use config_editor::ConfigSection;
use search::reindex::{self, ReindexKind};
use uuid::Uuid;
pub mod app;
pub mod audit;
pub mod config;
mod config_editor;
pub mod error;
mod exporter;
pub mod logging;
pub mod repository;
mod search;
#[cfg(test)]
pub mod test_support;
pub mod utils;
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ExportOptions {
    /// The Repository Config Types
    RepositoryConfigTypes,
    /// Export the repository types
    RepositoryTypes,
    /// Export the OpenAPI spec
    OpenAPI,
}
#[derive(Parser)]
#[command(
    version,
    about = "Repository Server CLI",
    long_about = "Github Repository: https://github.com/kshcherban/pkgly",
    author
)]
struct Command {
    #[clap(subcommand)]
    sub_command: SubCommands,
}
#[derive(Subcommand, Clone, Debug)]
enum SubCommands {
    /// Start the web server
    Start {
        /// The pkgly config file
        #[clap(short, long)]
        config: Option<PathBuf>,
    },
    #[cfg(feature = "frontend")]
    /// Validate the frontend files
    ///
    /// Makes sure the index.html file is present routes.json is valid
    ValidateFrontend,
    /// Save the default config file
    SaveConfig {
        /// The pkgly config file
        #[clap(short, long, default_value = "pkgly.toml")]
        config: PathBuf,
        /// If it should add defaults if the file already exists.
        #[clap(short, long, default_value = "false")]
        add_defaults: bool,
    },
    /// Opens an editor to edit the config file
    Config {
        /// The pkgly  config file
        #[clap(short, long, default_value = "pkgly.toml")]
        config: PathBuf,
        section: ConfigSection,
    },
    /// Export internal information
    Export {
        export: ExportOptions,
        location: PathBuf,
    },
    /// Search indexing and catalog maintenance commands
    Search {
        #[clap(subcommand)]
        command: SearchCommands,
    },
}

#[derive(Subcommand, Clone, Debug)]
enum SearchCommands {
    /// Reindex repository metadata used by the search service
    Reindex {
        #[clap(value_enum)]
        target: SearchReindexKind,
        /// Repository UUID to reindex
        #[clap(short, long)]
        repository: Uuid,
        /// Optional config file path (defaults to pkgly.toml)
        #[clap(short, long)]
        config: Option<PathBuf>,
    },
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum SearchReindexKind {
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

impl From<SearchReindexKind> for ReindexKind {
    fn from(value: SearchReindexKind) -> Self {
        match value {
            SearchReindexKind::NpmHosted => ReindexKind::NpmHosted,
            SearchReindexKind::NpmProxy => ReindexKind::NpmProxy,
            SearchReindexKind::PythonHosted => ReindexKind::PythonHosted,
            SearchReindexKind::PythonProxy => ReindexKind::PythonProxy,
            SearchReindexKind::MavenHosted => ReindexKind::MavenHosted,
            SearchReindexKind::MavenProxy => ReindexKind::MavenProxy,
            SearchReindexKind::PhpHosted => ReindexKind::PhpHosted,
            SearchReindexKind::PhpProxy => ReindexKind::PhpProxy,
            SearchReindexKind::GoHosted => ReindexKind::GoHosted,
            SearchReindexKind::GoProxy => ReindexKind::GoProxy,
            SearchReindexKind::DockerHosted => ReindexKind::DockerHosted,
            SearchReindexKind::DockerProxy => ReindexKind::DockerProxy,
            SearchReindexKind::CargoHosted => ReindexKind::CargoHosted,
            SearchReindexKind::HelmHosted => ReindexKind::HelmHosted,
            SearchReindexKind::DebHosted => ReindexKind::DebHosted,
            SearchReindexKind::DebProxy => ReindexKind::DebProxy,
            SearchReindexKind::RubyHosted => ReindexKind::RubyHosted,
        }
    }
}
fn main() -> anyhow::Result<()> {
    // For Some Reason Lettre fails if this is not installed
    if rustls::crypto::ring::default_provider()
        .install_default()
        .is_err()
    {
        eprintln!(
            "Default Crypto Provider already installed. This is not an error. But it should be reported."
        );
    }

    let command = Command::parse();

    match command.sub_command {
        SubCommands::Start { config } => web_start(config),
        SubCommands::SaveConfig {
            config,
            add_defaults,
        } => save_config(config, add_defaults),
        SubCommands::Export { export, location } => match export {
            ExportOptions::RepositoryConfigTypes => exporter::export_repository_configs(location),
            ExportOptions::RepositoryTypes => exporter::export_repository_types(location),
            ExportOptions::OpenAPI => exporter::export_openapi(location),
        },

        SubCommands::Config { config, section } => {
            let tokio = tokio::runtime::Builder::new_current_thread()
                .thread_name_fn(thread_name)
                .enable_all()
                .build()?;
            tokio.block_on(config_editor::editor(section, config))
        }
        SubCommands::Search { command } => run_search_command(command),

        #[cfg(feature = "frontend")]
        SubCommands::ValidateFrontend => {
            if let Err(error) = crate::app::frontend::HostedFrontend::validate() {
                eprintln!("Frontend Validation Failed: {error}");
                std::process::exit(1);
            } else {
                println!("Frontend Validation Successful");
            }
            Ok(())
        }
    }
}

fn web_start(config_path: Option<PathBuf>) -> anyhow::Result<()> {
    let config = load_config(config_path)?;
    let worker_threads = resolve_worker_threads(&config.web_server);
    let tokio = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .thread_name_fn(thread_name)
        .enable_all()
        .build()?;
    tokio.block_on(app::web::start_with_config(config))?;

    Ok(())
}
fn save_config(config_path: PathBuf, add_defaults: bool) -> anyhow::Result<()> {
    if config_path.exists() && !add_defaults {
        anyhow::bail!(
            "Config file already exists. Please remove it first. or use the --add-defaults flag to overwrite it."
        );
    }
    if config_path.is_dir() {
        anyhow::bail!("Config file is a directory. Please pass a file path.");
    }
    let config: PkglyConfig = if config_path.exists() {
        let config = std::fs::read_to_string(&config_path)?;
        toml::from_str(&config)?
    } else {
        PkglyConfig::default()
    };
    let contents = toml::to_string_pretty(&config)?;
    std::fs::write(config_path, contents)?;
    Ok(())
}

fn run_search_command(command: SearchCommands) -> anyhow::Result<()> {
    match command {
        SearchCommands::Reindex {
            target,
            repository,
            config,
        } => run_search_reindex(target, repository, config),
    }
}

fn run_search_reindex(
    target: SearchReindexKind,
    repository: Uuid,
    config: Option<PathBuf>,
) -> anyhow::Result<()> {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .thread_name_fn(thread_name)
        .enable_all()
        .build()?;

    let count = tokio.block_on(async move {
        let site = load_site_for_cli(config).await?;
        reindex::reindex_repository(site, repository, target.into()).await
    })?;

    println!("Reindex completed for repository {repository}: processed {count} artifact(s).");
    Ok(())
}

fn thread_name() -> String {
    static ATOMIC_ID: AtomicUsize = AtomicUsize::new(0);
    let id = ATOMIC_ID.fetch_add(1, Ordering::SeqCst);
    format!("pkgly-{}", id)
}

async fn load_site_for_cli(config_path: Option<PathBuf>) -> anyhow::Result<Pkgly> {
    let PkglyConfig {
        web_server: _,
        database,
        log: _,
        opentelemetry: _,
        mode,
        sessions,
        staging: staging_config,
        site,
        security,
        email,
        suggested_local_storage_path,
    } = load_config(config_path)?;

    let site = Pkgly::new(
        mode,
        site,
        security,
        sessions,
        staging_config,
        email,
        database,
        suggested_local_storage_path,
    )
    .await
    .context("Unable to initialize Pkgly site for CLI command")?;

    Ok(site)
}
