// ABOUTME: Provides the pkglyctl command runner and command-specific API calls.
// ABOUTME: Resolves CLI configuration, authentication, and repository operations.
pub mod cli;
pub mod config;
pub mod native;
pub mod output;
pub mod repo_ref;

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use bytes::Bytes;
use clap::Parser;
use config::{ConfigFile, ConfigOverrides, EnvConfig, ResolvedConfig};
use inquire::Password;
use nr_api::{
    Client, ClientConfig, CreateRepositoryRequest, CreateStorageRequest, CreateTokenRequest,
    PackageFileEntry, PackageListQuery, PackageListResponse,
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
    #[error(transparent)]
    Inquire(#[from] inquire::InquireError),
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
    run_with_password_prompt(cli, env, writer, prompt_password).await
}

pub async fn run_with_password_prompt<W, P>(
    cli: Cli,
    env: EnvConfig,
    writer: &mut W,
    mut password_prompt: P,
) -> Result<i32, CliError>
where
    W: Write,
    P: FnMut() -> Result<String, CliError>,
{
    let overrides = ConfigOverrides::from_global(&cli.global);
    let config_path = config::config_path(&overrides, &env);
    let mut config_file = ConfigFile::load_or_default(&config_path)?;
    let output = OutputFormat::from(cli.global.output.unwrap_or_default());

    match cli.command {
        Commands::Auth(command) => {
            handle_auth(
                command,
                AuthContext {
                    config_file: &mut config_file,
                    config_path: &config_path,
                    overrides: &overrides,
                    env: &env,
                    output,
                    writer,
                    password_prompt: &mut password_prompt,
                },
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

struct AuthContext<'a, W, P> {
    config_file: &'a mut ConfigFile,
    config_path: &'a Path,
    overrides: &'a ConfigOverrides,
    env: &'a EnvConfig,
    output: OutputFormat,
    writer: &'a mut W,
    password_prompt: &'a mut P,
}

async fn handle_auth<W: Write, P>(
    command: AuthCommands,
    context: AuthContext<'_, W, P>,
) -> Result<(), CliError>
where
    P: FnMut() -> Result<String, CliError>,
{
    match command {
        AuthCommands::Login {
            username,
            password,
            token_name,
        } => {
            let partial =
                ResolvedConfig::resolve(context.config_file, context.overrides, context.env)?;
            let base_url = partial.require_base_url()?;
            let client = Client::new(ClientConfig {
                base_url: base_url.clone(),
                token: None,
                user_agent: Some("pkglyctl".to_string()),
            })?;
            let password = match password {
                Some(value) => value,
                None => (context.password_prompt)()?,
            };
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
            let profile = context
                .config_file
                .profiles
                .entry(profile_name.clone())
                .or_default();
            profile.base_url = Some(base_url);
            profile.token = Some(token.token);
            context.config_file.active_profile = Some(profile_name);
            context.config_file.save(context.config_path)?;
            writeln!(context.writer, "login complete")?;
        }
        AuthCommands::SetToken { token, profile } => {
            let profile_name = profile
                .or_else(|| context.config_file.active_profile.clone())
                .unwrap_or_else(|| "local".to_string());
            context
                .config_file
                .upsert_profile_token(&profile_name, token);
            if context.config_file.active_profile.is_none() {
                context.config_file.active_profile = Some(profile_name);
            }
            context.config_file.save(context.config_path)?;
            writeln!(context.writer, "token saved")?;
        }
        AuthCommands::Whoami => {
            let resolved =
                ResolvedConfig::resolve(context.config_file, context.overrides, context.env)?
                    .require_complete()?;
            let client = client_from_resolved(&resolved)?;
            let me = client.whoami().await?;
            write!(
                context.writer,
                "{}",
                context.output.render_value(&serde_json::to_value(me)?)?
            )?;
        }
        AuthCommands::Logout => {
            let profile =
                ResolvedConfig::resolve(context.config_file, context.overrides, context.env)?
                    .profile
                    .or_else(|| context.config_file.active_profile.clone())
                    .ok_or_else(|| CliError::Message("no active profile".to_string()))?;
            if let Some(entry) = context.config_file.profiles.get_mut(&profile) {
                entry.token = None;
            }
            context.config_file.save(context.config_path)?;
            writeln!(context.writer, "token removed")?;
        }
    }
    Ok(())
}

fn prompt_password() -> Result<String, CliError> {
    if !std::io::stdin().is_terminal() {
        return Err(CliError::Message(
            "auth login requires --password when stdin is not interactive".to_string(),
        ));
    }
    Ok(Password::new("Password").without_confirmation().prompt()?)
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
        StorageCommands::Create {
            storage_type,
            name,
            path,
        } => {
            if storage_type != "local" {
                return Err(CliError::Message(
                    "only local storage is supported at the moment".to_string(),
                ));
            }
            let storage = client
                .create_storage(
                    &storage_type,
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
            query,
            no_header,
        } => {
            let id = resolve_repository_ref(client, &repository).await?;
            let packages = list_all_package_entries(client, id, query).await?;
            let rows = package_summary_rows(&packages);
            let rendered = if no_header && output == OutputFormat::Table {
                render_rows_without_header(&rows)
            } else {
                output.render_rows(&["Package", "Version", "Blob", "Size", "Modified"], &rows)
            };
            write!(writer, "{rendered}")?;
        }
        PackageCommands::Describe {
            repository,
            package,
            version,
        } => {
            let id = resolve_repository_ref(client, &repository).await?;
            let packages = client
                .list_packages(id, &package_list_query(Some(package.clone())))
                .await?;
            let selected = select_package_entry(&packages.items, &package, version.as_deref())?;
            write!(
                writer,
                "{}",
                output.render_value(&package_entry_display_value(&selected)?)?
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

fn package_list_query(query: Option<String>) -> PackageListQuery {
    package_page_query(1, query)
}

fn package_page_query(page: usize, query: Option<String>) -> PackageListQuery {
    PackageListQuery {
        page,
        per_page: 1000,
        q: query,
        ..PackageListQuery::default()
    }
}

async fn list_all_package_entries(
    client: &Client,
    repository_id: Uuid,
    query: Option<String>,
) -> Result<Vec<PackageFileEntry>, CliError> {
    let mut page = 1;
    let mut items = Vec::new();

    loop {
        let response = client
            .list_packages(repository_id, &package_page_query(page, query.clone()))
            .await?;
        let done = package_listing_is_complete(&response, items.len());
        let page_was_empty = response.items.is_empty();
        items.extend(response.items);
        if done || page_was_empty {
            break;
        }
        page += 1;
    }

    Ok(items)
}

fn package_listing_is_complete(response: &PackageListResponse, previous_count: usize) -> bool {
    previous_count + response.items.len() >= response.total_packages
}

fn package_summary_rows(items: &[PackageFileEntry]) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for item in items {
        let row = vec![
            item.package.clone(),
            item.name.clone(),
            item.blob_digest.clone().unwrap_or_default(),
            format_bytes(item.size),
            item.modified.to_rfc3339(),
        ];
        if !rows.contains(&row) {
            rows.push(row);
        }
    }
    rows
}

fn render_rows_without_header(rows: &[Vec<String>]) -> String {
    let mut output = String::new();
    for row in rows {
        output.push_str(&row.join("  "));
        output.push('\n');
    }
    output
}

fn package_entry_display_value(entry: &PackageFileEntry) -> Result<Value, serde_json::Error> {
    let mut value = serde_json::to_value(entry)?;
    if let Value::Object(object) = &mut value {
        object.insert("size".to_string(), Value::String(format_bytes(entry.size)));
    }
    Ok(value)
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }

    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }
    format!("{size:.1} {}", UNITS[unit])
}

fn select_package_entry(
    items: &[PackageFileEntry],
    package: &str,
    version: Option<&str>,
) -> Result<PackageFileEntry, CliError> {
    items
        .iter()
        .find(|item| {
            item.package == package && version.map(|value| item.name == value).unwrap_or(true)
        })
        .cloned()
        .ok_or_else(|| {
            let target = version
                .map(|value| format!("{package} {value}"))
                .unwrap_or_else(|| package.to_string());
            CliError::Message(format!("package `{target}` not found"))
        })
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
