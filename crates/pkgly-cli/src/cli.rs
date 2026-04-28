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
    #[command(subcommand)]
    Auth(AuthCommands),
    #[command(subcommand)]
    Profile(ProfileCommands),
    #[command(subcommand)]
    Storage(StorageCommands),
    #[command(subcommand)]
    Repo(RepoCommands),
    #[command(subcommand)]
    Package(PackageCommands),
    #[command(subcommand)]
    Native(NativeCommands),
}

#[derive(Debug, Subcommand)]
pub enum AuthCommands {
    Login {
        #[arg(long)]
        username: String,
        #[arg(long)]
        password: Option<String>,
        #[arg(long)]
        token_name: Option<String>,
    },
    SetToken {
        token: String,
        #[arg(long)]
        profile: Option<String>,
    },
    Whoami,
    Logout,
}

#[derive(Debug, Subcommand)]
pub enum ProfileCommands {
    List,
    Show { profile: Option<String> },
    Use { profile: String },
    Remove { profile: String },
}

#[derive(Debug, Subcommand)]
pub enum StorageCommands {
    List,
    Get { id: Uuid },
    CreateLocal { name: String, path: String },
}

#[derive(Debug, Subcommand)]
pub enum RepoCommands {
    List,
    Get {
        repository: String,
    },
    Id {
        repository: String,
    },
    Create {
        repository_type: String,
        name: String,
        #[arg(long)]
        storage: Option<String>,
        #[arg(long)]
        config: Option<String>,
    },
    Delete {
        repository: String,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        force: bool,
    },
    ConfigList {
        repository: String,
    },
    ConfigGet {
        repository: String,
        key: String,
    },
    ConfigSet {
        repository: String,
        key: String,
        value: String,
    },
    Url {
        repository: String,
        path: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum PackageCommands {
    List {
        repository: String,
        #[arg(long, default_value_t = 1)]
        page: usize,
        #[arg(long, default_value_t = 50)]
        per_page: usize,
        #[arg(long)]
        query: Option<String>,
    },
    Search {
        query: String,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    Browse {
        repository: String,
        path: Option<String>,
    },
    Download {
        repository: String,
        path: String,
        #[arg(long)]
        output_file: Option<PathBuf>,
    },
    Delete {
        repository: String,
        paths: Vec<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        force: bool,
    },
    #[command(subcommand)]
    Upload(PackageUploadCommands),
}

#[derive(Debug, Subcommand)]
pub enum PackageUploadCommands {
    Maven {
        repository: String,
        path: String,
        file: PathBuf,
    },
    Python {
        repository: String,
        name: String,
        version: String,
        file: PathBuf,
    },
    Go {
        repository: String,
        module_name: String,
        version: String,
        module: PathBuf,
        info: PathBuf,
        gomod: PathBuf,
    },
    Php {
        repository: String,
        dist_path: String,
        file: PathBuf,
    },
    Deb {
        repository: String,
        path: String,
        file: PathBuf,
    },
    Helm {
        repository: String,
        path: String,
        file: PathBuf,
    },
    Ruby {
        repository: String,
        file: PathBuf,
    },
    Nuget {
        repository: String,
        file: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum NativeCommands {
    Npm {
        repository: String,
    },
    Cargo {
        repository: String,
    },
    Docker {
        repository: String,
        image: Option<String>,
    },
}
