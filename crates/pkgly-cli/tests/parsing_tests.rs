// ABOUTME: Verifies pkglyctl argument parsing and small formatting helpers.
// ABOUTME: Keeps command-line contracts covered without calling a server.
use clap::{CommandFactory, Parser};
use uuid::Uuid;

use pkgly_cli::{
    cli::{Cli, Commands, OutputMode, RepoCommands, StorageCommands},
    output::{OutputFormat, redact_secret},
    repo_ref::RepositoryRef,
};

#[test]
fn repository_ref_accepts_uuid_or_storage_repository_name() {
    let id = Uuid::nil();
    assert_eq!(
        RepositoryRef::parse(&id.to_string()),
        Ok(RepositoryRef::Id(id))
    );
    assert_eq!(
        RepositoryRef::parse("test-storage/releases"),
        Ok(RepositoryRef::Names {
            storage: "test-storage".to_string(),
            repository: "releases".to_string(),
        })
    );
    assert!(RepositoryRef::parse("missing-slash").is_err());
}

#[test]
fn cli_parses_global_flags_and_subcommands() {
    let cli = Cli::try_parse_from([
        "pkglyctl",
        "--profile",
        "local",
        "--output",
        "json",
        "repo",
        "get",
        "test/repo",
    ])
    .unwrap_or_else(|err| panic!("failed to parse cli: {err}"));

    assert_eq!(cli.global.profile.as_deref(), Some("local"));
    assert_eq!(cli.global.output, Some(OutputMode::Json));
    assert!(matches!(
        cli.command,
        Commands::Repo(RepoCommands::Get { ref repository }) if repository == "test/repo"
    ));
}

#[test]
fn cli_parses_storage_create_with_local_type() {
    let cli = Cli::try_parse_from([
        "pkglyctl",
        "storage",
        "create",
        "--type",
        "local",
        "test-storage",
        "/var/lib/pkgly/storage",
    ])
    .unwrap_or_else(|err| panic!("failed to parse cli: {err}"));

    assert!(matches!(
        cli.command,
        Commands::Storage(StorageCommands::Create {
            ref storage_type,
            ref name,
            ref path,
        }) if storage_type == "local"
            && name == "test-storage"
            && path == "/var/lib/pkgly/storage"
    ));
}

#[test]
fn cli_parses_package_describe_with_optional_version() {
    let cli = Cli::try_parse_from([
        "pkglyctl",
        "package",
        "describe",
        "local/deb",
        "wget",
        "1.25.0-2",
    ])
    .unwrap_or_else(|err| panic!("failed to parse cli: {err}"));

    assert!(matches!(
        cli.command,
        Commands::Package(pkgly_cli::cli::PackageCommands::Describe {
            ref repository,
            ref package,
            ref version,
        }) if repository == "local/deb"
            && package == "wget"
            && version.as_deref() == Some("1.25.0-2")
    ));
}

#[test]
fn storage_create_help_lists_supported_type() {
    let help = Cli::command()
        .find_subcommand_mut("storage")
        .and_then(|command| command.find_subcommand_mut("create"))
        .map(|command| command.render_help().to_string())
        .unwrap_or_else(|| panic!("storage create help missing"));

    assert!(help.contains("--type <STORAGE_TYPE>"));
    assert!(help.contains("Only local storage is supported"));
}

#[test]
fn top_level_help_describes_command_groups() {
    let help = Cli::command().render_help().to_string();

    assert!(help.contains("auth"));
    assert!(help.contains("Authenticate and manage API tokens"));
    assert!(help.contains("storage"));
    assert!(help.contains("Manage storage backends"));
    assert!(help.contains("repo"));
    assert!(help.contains("Manage repositories"));
}

#[test]
fn repo_help_describes_subcommands() {
    let help = Cli::command()
        .find_subcommand_mut("repo")
        .map(|command| command.render_help().to_string())
        .unwrap_or_else(|| panic!("repo help missing"));

    assert!(help.contains("List repositories"));
    assert!(help.contains("Show repository details"));
    assert!(help.contains("Create a repository"));
    assert!(help.contains("Delete a repository"));
    assert!(help.contains("Build a repository route URL"));
}

#[test]
fn package_help_describes_describe_command() {
    let help = Cli::command()
        .find_subcommand_mut("package")
        .map(|command| command.render_help().to_string())
        .unwrap_or_else(|| panic!("package help missing"));

    assert!(help.contains("List package names and versions"));
    assert!(help.contains("Show package details"));
}

#[test]
fn package_upload_help_describes_subcommands() {
    let help = Cli::command()
        .find_subcommand_mut("package")
        .and_then(|command| command.find_subcommand_mut("upload"))
        .map(|command| command.render_help().to_string())
        .unwrap_or_else(|| panic!("package upload help missing"));

    assert!(help.contains("Upload a Maven artifact"));
    assert!(help.contains("Upload a Python distribution"));
    assert!(help.contains("Upload a NuGet package"));
}

#[test]
fn output_json_is_machine_readable_and_table_is_plain_text() {
    let value = serde_json::json!({"name": "repo", "active": true});
    let json = OutputFormat::Json
        .render_value(&value)
        .unwrap_or_else(|err| panic!("json render failed: {err}"));
    assert_eq!(json, "{\"active\":true,\"name\":\"repo\"}\n");

    let table =
        OutputFormat::Table.render_rows(&["Name", "Active"], &[vec!["repo".into(), "true".into()]]);
    assert_eq!(table, "Name  Active\nrepo  true\n");
}

#[test]
fn secret_redaction_keeps_short_context_only() {
    assert_eq!(redact_secret(""), "");
    assert_eq!(redact_secret("abc"), "***");
    assert_eq!(redact_secret("abcdefghijklmnopqrstuvwxyz"), "abcd...wxyz");
}
