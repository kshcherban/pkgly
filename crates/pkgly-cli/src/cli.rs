// ABOUTME: Defines the pkglyctl command-line argument tree and parsed commands.
// ABOUTME: Keeps clap metadata close to the public CLI contract.
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(
    name = "pkglyctl",
    version,
    about = "Operate a Pkgly server from the terminal"
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Args, Default)]
pub struct GlobalArgs {
    #[arg(long, global = true)]
    pub profile: Option<String>,
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    #[arg(long, global = true)]
    pub base_url: Option<String>,
    #[arg(long, global = true)]
    pub token: Option<String>,
    #[arg(long, value_enum, global = true)]
    pub output: Option<OutputMode>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputMode {
    #[default]
    Table,
    Json,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(subcommand, about = "Authenticate and manage API tokens")]
    Auth(AuthCommands),
    #[command(subcommand, about = "Manage CLI profiles")]
    Profile(ProfileCommands),
    #[command(subcommand, about = "Manage storage backends")]
    Storage(StorageCommands),
    #[command(subcommand, about = "Manage repositories")]
    Repo(RepoCommands),
    #[command(subcommand, about = "List, search, download, and upload packages")]
    Package(PackageCommands),
    #[command(subcommand, about = "Print native package manager commands")]
    Native(NativeCommands),
}

#[derive(Debug, Subcommand)]
pub enum AuthCommands {
    #[command(about = "Log in with username and password and store an API token")]
    Login {
        #[arg(long)]
        username: String,
        #[arg(long)]
        password: Option<String>,
        #[arg(long)]
        token_name: Option<String>,
    },
    #[command(about = "Store an existing API token in a profile")]
    SetToken {
        token: String,
        #[arg(long)]
        profile: Option<String>,
    },
    #[command(about = "Show the authenticated user")]
    Whoami,
    #[command(about = "Remove the stored token from the active profile")]
    Logout,
}

#[derive(Debug, Subcommand)]
pub enum ProfileCommands {
    #[command(about = "List configured profiles")]
    List,
    #[command(about = "Show a profile configuration")]
    Show { profile: Option<String> },
    #[command(about = "Set the active profile")]
    Use { profile: String },
    #[command(about = "Remove a profile")]
    Remove { profile: String },
}

#[derive(Debug, Subcommand)]
pub enum StorageCommands {
    #[command(about = "List storage backends")]
    List,
    #[command(about = "Show storage backend details")]
    Get { id: Uuid },
    #[command(about = "Create a storage backend")]
    Create {
        #[arg(
            long = "type",
            default_value = "local",
            help = "Storage type to create. Only local storage is supported at the moment."
        )]
        storage_type: String,
        name: String,
        path: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum RepoCommands {
    #[command(about = "List repositories")]
    List,
    #[command(about = "Show repository details")]
    Get { repository: String },
    #[command(about = "Resolve a repository reference to its UUID")]
    Id { repository: String },
    #[command(about = "Create a repository")]
    Create {
        repository_type: String,
        name: String,
        #[arg(long)]
        storage: Option<String>,
        #[arg(long)]
        config: Option<String>,
    },
    #[command(about = "Delete a repository")]
    Delete {
        repository: String,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        force: bool,
    },
    #[command(about = "List repository config keys")]
    ConfigList { repository: String },
    #[command(about = "Show a repository config value")]
    ConfigGet { repository: String, key: String },
    #[command(about = "Set a repository config value from JSON")]
    ConfigSet {
        repository: String,
        key: String,
        value: String,
    },
    #[command(about = "Build a repository route URL")]
    Url {
        repository: String,
        path: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum PackageCommands {
    #[command(about = "List package names and versions")]
    List {
        repository: String,
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        no_header: bool,
    },
    #[command(about = "Show package details")]
    Describe {
        repository: String,
        package: String,
        version: Option<String>,
    },
    #[command(about = "Search packages across repositories")]
    Search {
        query: String,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    #[command(about = "Browse package paths in a repository")]
    Browse {
        repository: String,
        path: Option<String>,
    },
    #[command(about = "Download a package file")]
    Download {
        repository: String,
        path: String,
        #[arg(long)]
        output_file: Option<PathBuf>,
    },
    #[command(about = "Delete package paths")]
    Delete {
        repository: String,
        paths: Vec<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        force: bool,
    },
    #[command(subcommand, about = "Upload package files")]
    Upload(PackageUploadCommands),
}

#[derive(Debug, Subcommand)]
pub enum PackageUploadCommands {
    #[command(about = "Upload a Maven artifact")]
    Maven {
        repository: String,
        path: String,
        file: PathBuf,
    },
    #[command(about = "Upload a Python distribution")]
    Python {
        repository: String,
        name: String,
        version: String,
        file: PathBuf,
    },
    #[command(about = "Upload a Go module")]
    Go {
        repository: String,
        module_name: String,
        version: String,
        module: PathBuf,
        info: PathBuf,
        gomod: PathBuf,
    },
    #[command(about = "Upload a PHP package archive")]
    Php {
        repository: String,
        dist_path: String,
        file: PathBuf,
    },
    #[command(about = "Upload a Debian package")]
    Deb {
        repository: String,
        path: String,
        file: PathBuf,
    },
    #[command(about = "Upload a Helm chart")]
    Helm {
        repository: String,
        path: String,
        file: PathBuf,
    },
    #[command(about = "Upload a RubyGem")]
    Ruby { repository: String, file: PathBuf },
    #[command(about = "Upload a NuGet package")]
    Nuget { repository: String, file: PathBuf },
}

#[derive(Debug, Subcommand)]
pub enum NativeCommands {
    #[command(about = "Print npm registry commands")]
    Npm { repository: String },
    #[command(about = "Print Cargo registry commands")]
    Cargo { repository: String },
    #[command(about = "Print Docker registry commands")]
    Docker {
        repository: String,
        image: Option<String>,
    },
}
