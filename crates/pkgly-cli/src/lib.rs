pub mod cli;
pub mod config;
pub mod native;
pub mod output;
pub mod repo_ref;

use std::io::{IsTerminal, Write};
use std::path::PathBuf;

use bytes::Bytes;
use clap::Parser;
use config::{ConfigFile, ConfigOverrides, EnvConfig, ResolvedConfig};
use nr_api::{
    Client, ClientConfig, CreateRepositoryRequest, CreateStorageRequest, CreateTokenRequest,
    PackageListQuery,
};
use output::{OutputFormat, render_json_pretty};
use repo_ref::RepositoryRef;
use serde_json::{Value, json};
use thiserror::Error;
use uuid::Uuid;

use crate::cli::{
    AuthCommands, Cli, Commands, PackageCommands, PackageUploadCommands, ProfileCommands,
    RepoCommands, StorageCommands,
};

#[derive(Debug, Error)]
pub enum CliError {
    #[error(transparent)]
    Config(#[from] config::ConfigError),
    #[error(transparent)]
    Api(#[from] nr_api::Error),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Uuid(#[from] uuid::Error),
    #[error("{0}")]
    Message(String),
}

pub async fn run_from_args() -> Result<i32, CliError> {
    let cli = Cli::parse();
    let env = EnvConfig::from_process();
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    run(cli, env, &mut handle).await
}

pub async fn run<W: Write>(cli: Cli, env: EnvConfig, writer: &mut W) -> Result<i32, CliError> {
    let overrides = ConfigOverrides::from_global(&cli.global);
    let config_path = config::config_path(&overrides, &env);
    let mut config_file = ConfigFile::load_or_default(&config_path)?;
    let output = OutputFormat::from(cli.global.output.unwrap_or_default());

    match cli.command {
        Commands::Auth(command) => {
            handle_auth(
                command,
                &mut config_file,
                &config_path,
                &overrides,
                &env,
                output,
                writer,
            )
            .await?;
        }
        Commands::Profile(command) => {
            handle_profile(command, &mut config_file, &config_path, output, writer)?;
        }
        Commands::Native(command) => {
            let resolved = ResolvedConfig::resolve(&config_file, &overrides, &env)?;
            let text = native::render(command, &resolved)?;
            writeln!(writer, "{text}")?;
        }
        command => {
            let resolved =
                ResolvedConfig::resolve(&config_file, &overrides, &env)?.require_complete()?;
            let client = client_from_resolved(&resolved)?;
            handle_remote(command, &client, &resolved, output, writer).await?;
        }
    }

    Ok(0)
}

async fn handle_auth<W: Write>(
    command: AuthCommands,
    config_file: &mut ConfigFile,
    config_path: &std::path::Path,
    overrides: &ConfigOverrides,
    env: &EnvConfig,
    output: OutputFormat,
    writer: &mut W,
) -> Result<(), CliError> {
    match command {
        AuthCommands::Login {
            username,
            password,
            token_name,
        } => {
            let partial = ResolvedConfig::resolve(config_file, overrides, env)?;
            let base_url = partial.require_base_url()?;
            let client = Client::new(ClientConfig {
                base_url,
                token: None,
                user_agent: Some("pkglyctl".to_string()),
            })?;
            let password = password.ok_or_else(|| {
                CliError::Message(
                    "auth login requires --password for non-interactive v1".to_string(),
                )
            })?;
            let session = client.login(&username, &password).await?;
            let token = client
                .create_token_with_session(
                    &session,
                    &CreateTokenRequest {
                        name: token_name.or_else(|| Some("pkglyctl".to_string())),
                        description: Some("Created by pkglyctl auth login".to_string()),
                        expires_in_days: None,
                        scopes: vec![
                            "ReadRepository".to_string(),
                            "WriteRepository".to_string(),
                            "EditRepository".to_string(),
                        ],
                        repository_scopes: Vec::new(),
                    },
                )
                .await?;
            let profile_name = partial
                .profile
                .clone()
                .unwrap_or_else(|| "local".to_string());
            config_file.upsert_profile_token(&profile_name, token.token);
            config_file.active_profile = Some(profile_name);
            config_file.save(config_path)?;
            writeln!(writer, "login complete")?;
        }
        AuthCommands::SetToken { token, profile } => {
            let profile_name = profile
                .or_else(|| config_file.active_profile.clone())
                .unwrap_or_else(|| "local".to_string());
            config_file.upsert_profile_token(&profile_name, token);
            if config_file.active_profile.is_none() {
                config_file.active_profile = Some(profile_name);
            }
            config_file.save(config_path)?;
            writeln!(writer, "token saved")?;
        }
        AuthCommands::Whoami => {
            let resolved =
                ResolvedConfig::resolve(config_file, overrides, env)?.require_complete()?;
            let client = client_from_resolved(&resolved)?;
            let me = client.whoami().await?;
            write!(
                writer,
                "{}",
                output.render_value(&serde_json::to_value(me)?)?
            )?;
        }
        AuthCommands::Logout => {
            let profile = ResolvedConfig::resolve(config_file, overrides, env)?
                .profile
                .or_else(|| config_file.active_profile.clone())
                .ok_or_else(|| CliError::Message("no active profile".to_string()))?;
            if let Some(entry) = config_file.profiles.get_mut(&profile) {
                entry.token = None;
            }
            config_file.save(config_path)?;
            writeln!(writer, "token removed")?;
        }
    }
    Ok(())
}

fn handle_profile<W: Write>(
    command: ProfileCommands,
    config_file: &mut ConfigFile,
    config_path: &std::path::Path,
    output: OutputFormat,
    writer: &mut W,
) -> Result<(), CliError> {
    match command {
        ProfileCommands::List => {
            let rows = config_file
                .profiles
                .iter()
                .map(|(name, profile)| {
                    vec![
                        marker(config_file.active_profile.as_deref() == Some(name)),
                        name.clone(),
                        profile.base_url.clone().unwrap_or_default(),
                        marker(profile.token.is_some()),
                    ]
                })
                .collect::<Vec<_>>();
            write!(
                writer,
                "{}",
                output.render_rows(&["Active", "Name", "Base URL", "Token"], &rows)
            )?;
        }
        ProfileCommands::Show { profile } => {
            let name = profile
                .or_else(|| config_file.active_profile.clone())
                .ok_or_else(|| CliError::Message("no active profile".to_string()))?;
            let profile = config_file
                .profiles
                .get(&name)
                .ok_or_else(|| CliError::Message(format!("profile `{name}` not found")))?;
            let value = json!({
                "name": name,
                "base_url": profile.base_url,
                "token": profile.token.as_deref().map(output::redact_secret),
                "default_storage": profile.default_storage,
            });
            write!(writer, "{}", output.render_value(&value)?)?;
        }
        ProfileCommands::Use { profile } => {
            config::ProfileMutation::Use(profile).apply(config_file)?;
            config_file.save(config_path)?;
            writeln!(writer, "active profile updated")?;
        }
        ProfileCommands::Remove { profile } => {
            config::ProfileMutation::Remove(profile).apply(config_file)?;
            config_file.save(config_path)?;
            writeln!(writer, "profile removed")?;
        }
    }
    Ok(())
}

async fn handle_remote<W: Write>(
    command: Commands,
    client: &Client,
    resolved: &ResolvedConfig,
    output: OutputFormat,
    writer: &mut W,
) -> Result<(), CliError> {
    match command {
        Commands::Storage(command) => handle_storage(command, client, output, writer).await,
        Commands::Repo(command) => handle_repo(command, client, resolved, output, writer).await,
        Commands::Package(command) => handle_package(command, client, output, writer).await,
        Commands::Auth(_) | Commands::Profile(_) | Commands::Native(_) => Ok(()),
    }
}

async fn handle_storage<W: Write>(
    command: StorageCommands,
    client: &Client,
    output: OutputFormat,
    writer: &mut W,
) -> Result<(), CliError> {
    match command {
        StorageCommands::List => {
            let storages = client.list_storages().await?;
            write!(
                writer,
                "{}",
                output.render_value(&serde_json::to_value(storages)?)?
            )?;
        }
        StorageCommands::Get { id } => {
            let storage = client.get_storage(id).await?;
            write!(
                writer,
                "{}",
                output.render_value(&serde_json::to_value(storage)?)?
            )?;
        }
        StorageCommands::CreateLocal { name, path } => {
            let storage = client
                .create_storage(
                    "local",
                    &CreateStorageRequest {
                        name,
                        config: json!({"type": "Local", "settings": {"path": path}}),
                    },
                )
                .await?;
            write!(
                writer,
                "{}",
                output.render_value(&serde_json::to_value(storage)?)?
            )?;
        }
    }
    Ok(())
}

async fn handle_repo<W: Write>(
    command: RepoCommands,
    client: &Client,
    resolved: &ResolvedConfig,
    output: OutputFormat,
    writer: &mut W,
) -> Result<(), CliError> {
    match command {
        RepoCommands::List => {
            let repos = client.list_repositories().await?;
            write!(
                writer,
                "{}",
                output.render_value(&serde_json::to_value(repos)?)?
            )?;
        }
        RepoCommands::Get { repository } => {
            let id = resolve_repository_ref(client, &repository).await?;
            let repo = client.get_repository(id).await?;
            write!(
                writer,
                "{}",
                output.render_value(&serde_json::to_value(repo)?)?
            )?;
        }
        RepoCommands::Id { repository } => {
            let id = resolve_repository_ref(client, &repository).await?;
            writeln!(writer, "{id}")?;
        }
        RepoCommands::Create {
            repository_type,
            name,
            storage,
            config,
        } => {
            let configs = parse_json_map(config.as_deref())?;
            let storage_name = storage.or_else(|| resolved.default_storage.clone());
            let repo = client
                .create_repository(
                    &repository_type,
                    &CreateRepositoryRequest {
                        name,
                        storage: None,
                        storage_name,
                        configs,
                    },
                )
                .await?;
            write!(
                writer,
                "{}",
                output.render_value(&serde_json::to_value(repo)?)?
            )?;
        }
        RepoCommands::Delete {
            repository,
            yes,
            force,
        } => {
            require_destructive_confirmation(yes, force)?;
            let id = resolve_repository_ref(client, &repository).await?;
            client.delete_repository(id).await?;
            writeln!(writer, "repository deleted")?;
        }
        RepoCommands::ConfigList { repository } => {
            let id = resolve_repository_ref(client, &repository).await?;
            let configs = client.list_repository_configs(id).await?;
            write!(
                writer,
                "{}",
                output.render_value(&serde_json::to_value(configs)?)?
            )?;
        }
        RepoCommands::ConfigGet { repository, key } => {
            let id = resolve_repository_ref(client, &repository).await?;
            let value = client.get_repository_config(id, &key).await?;
            write!(writer, "{}", output.render_value(&value)?)?;
        }
        RepoCommands::ConfigSet {
            repository,
            key,
            value,
        } => {
            let id = resolve_repository_ref(client, &repository).await?;
            let value: Value = serde_json::from_str(&value)?;
            client.set_repository_config(id, &key, &value).await?;
            writeln!(writer, "config updated")?;
        }
        RepoCommands::Url { repository, path } => {
            let repo_ref = RepositoryRef::parse(&repository)?;
            let (storage, repo) = match repo_ref {
                RepositoryRef::Names {
                    storage,
                    repository,
                } => (storage, repository),
                RepositoryRef::Id(id) => {
                    let repo = client.get_repository(id).await?;
                    (repo.storage_name, repo.name)
                }
            };
            let url =
                client.repository_url(&format!("{storage}/{repo}/{}", path.unwrap_or_default()))?;
            writeln!(writer, "{url}")?;
        }
    }
    Ok(())
}

async fn handle_package<W: Write>(
    command: PackageCommands,
    client: &Client,
    output: OutputFormat,
    writer: &mut W,
) -> Result<(), CliError> {
    match command {
        PackageCommands::List {
            repository,
            page,
            per_page,
            query,
        } => {
            let id = resolve_repository_ref(client, &repository).await?;
            let packages = client
                .list_packages(
                    id,
                    &PackageListQuery {
                        page,
                        per_page,
                        q: query,
                        ..PackageListQuery::default()
                    },
                )
                .await?;
            write!(
                writer,
                "{}",
                output.render_value(&serde_json::to_value(packages)?)?
            )?;
        }
        PackageCommands::Search { query, limit } => {
            let results = client.search_packages(&query, limit).await?;
            write!(
                writer,
                "{}",
                output.render_value(&serde_json::to_value(results)?)?
            )?;
        }
        PackageCommands::Browse { repository, path } => {
            let repo = repository_names(client, &repository).await?;
            let url = client.repository_url(&format!(
                "{}/{}/{}",
                repo.0,
                repo.1,
                path.unwrap_or_default()
            ))?;
            writeln!(writer, "{url}")?;
        }
        PackageCommands::Download {
            repository,
            path,
            output_file,
        } => {
            let (storage, repo) = repository_names(client, &repository).await?;
            let response = client
                .download_repository_path(&storage, &repo, &path)
                .await?;
            let bytes = response.bytes().await?;
            if let Some(output_file) = output_file {
                tokio::fs::write(output_file, bytes).await?;
            } else {
                writer.write_all(&bytes)?;
            }
        }
        PackageCommands::Delete {
            repository,
            paths,
            yes,
            force,
        } => {
            require_destructive_confirmation(yes, force)?;
            let id = resolve_repository_ref(client, &repository).await?;
            let deleted = client.delete_packages(id, &paths).await?;
            write!(
                writer,
                "{}",
                output.render_value(&serde_json::to_value(deleted)?)?
            )?;
        }
        PackageCommands::Upload(command) => {
            handle_upload(command, client).await?;
            writeln!(writer, "upload complete")?;
        }
    }
    Ok(())
}

async fn handle_upload(command: PackageUploadCommands, client: &Client) -> Result<(), CliError> {
    match command {
        PackageUploadCommands::Maven {
            repository,
            path,
            file,
        } => {
            let (storage, repo) = repository_names(client, &repository).await?;
            client
                .put_repository_bytes(&storage, &repo, &path, read_file_bytes(file).await?)
                .await?;
        }
        PackageUploadCommands::Python {
            repository,
            name,
            version,
            file,
        } => {
            let (storage, repo) = repository_names(client, &repository).await?;
            let file_name = file_name(&file)?;
            let form = reqwest::multipart::Form::new()
                .text("name", name)
                .text("version", version)
                .part(
                    "content",
                    reqwest::multipart::Part::bytes(read_file_vec(file).await?)
                        .file_name(file_name),
                );
            client
                .post_repository_multipart(&storage, &repo, "", form)
                .await?;
        }
        PackageUploadCommands::Go {
            repository,
            module_name,
            version,
            module,
            info,
            gomod,
        } => {
            let (storage, repo) = repository_names(client, &repository).await?;
            let form = reqwest::multipart::Form::new()
                .text("module_name", module_name)
                .text("version", version)
                .part(
                    "module",
                    reqwest::multipart::Part::bytes(read_file_vec(module).await?),
                )
                .part(
                    "info",
                    reqwest::multipart::Part::bytes(read_file_vec(info).await?),
                )
                .part(
                    "gomod",
                    reqwest::multipart::Part::bytes(read_file_vec(gomod).await?),
                );
            client
                .post_repository_multipart(&storage, &repo, "upload", form)
                .await?;
        }
        PackageUploadCommands::Php {
            repository,
            dist_path,
            file,
        }
        | PackageUploadCommands::Deb {
            repository,
            path: dist_path,
            file,
        }
        | PackageUploadCommands::Helm {
            repository,
            path: dist_path,
            file,
        } => {
            let (storage, repo) = repository_names(client, &repository).await?;
            client
                .put_repository_bytes(&storage, &repo, &dist_path, read_file_bytes(file).await?)
                .await?;
        }
        PackageUploadCommands::Ruby { repository, file } => {
            let (storage, repo) = repository_names(client, &repository).await?;
            client
                .post_repository_bytes(&storage, &repo, "api/v1/gems", read_file_bytes(file).await?)
                .await?;
        }
        PackageUploadCommands::Nuget { repository, file } => {
            let (storage, repo) = repository_names(client, &repository).await?;
            let file_name = file_name(&file)?;
            let form = reqwest::multipart::Form::new().part(
                "package",
                reqwest::multipart::Part::bytes(read_file_vec(file).await?).file_name(file_name),
            );
            client
                .post_repository_multipart(&storage, &repo, "", form)
                .await?;
        }
    }
    Ok(())
}

fn client_from_resolved(resolved: &ResolvedConfig) -> Result<Client, CliError> {
    Ok(Client::new(ClientConfig {
        base_url: resolved.base_url.clone(),
        token: resolved.token.clone(),
        user_agent: Some("pkglyctl".to_string()),
    })?)
}

async fn resolve_repository_ref(client: &Client, value: &str) -> Result<Uuid, CliError> {
    match RepositoryRef::parse(value)? {
        RepositoryRef::Id(id) => Ok(id),
        RepositoryRef::Names {
            storage,
            repository,
        } => Ok(client
            .find_repository_id(&storage, &repository)
            .await?
            .repository_id),
    }
}

async fn repository_names(client: &Client, value: &str) -> Result<(String, String), CliError> {
    match RepositoryRef::parse(value)? {
        RepositoryRef::Names {
            storage,
            repository,
        } => Ok((storage, repository)),
        RepositoryRef::Id(id) => {
            let repository = client.get_repository(id).await?;
            Ok((repository.storage_name, repository.name))
        }
    }
}

fn parse_json_map(value: Option<&str>) -> Result<serde_json::Map<String, Value>, CliError> {
    let Some(value) = value else {
        return Ok(Default::default());
    };
    let parsed: Value = serde_json::from_str(value)?;
    match parsed {
        Value::Object(map) => Ok(map),
        _ => Err(CliError::Message(
            "config must be a JSON object".to_string(),
        )),
    }
}

fn require_destructive_confirmation(yes: bool, force: bool) -> Result<(), CliError> {
    if yes {
        return Ok(());
    }
    if force && !std::io::stdin().is_terminal() {
        return Ok(());
    }
    Err(CliError::Message(
        "destructive command requires --yes, or --force with non-interactive stdin".to_string(),
    ))
}

async fn read_file_bytes(path: PathBuf) -> Result<Bytes, CliError> {
    Ok(Bytes::from(read_file_vec(path).await?))
}

async fn read_file_vec(path: PathBuf) -> Result<Vec<u8>, CliError> {
    Ok(tokio::fs::read(path).await?)
}

fn file_name(path: &std::path::Path) -> Result<String, CliError> {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            CliError::Message(format!("file `{}` has no valid file name", path.display()))
        })
}

fn marker(value: bool) -> String {
    if value {
        "yes".to_string()
    } else {
        "no".to_string()
    }
}

pub fn render_value_for_tests(value: &Value) -> Result<String, CliError> {
    Ok(render_json_pretty(value)?)
}
