use clap::Parser;
use uuid::Uuid;

use pkgly_cli::{
    cli::{Cli, Commands, OutputMode, RepoCommands},
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
