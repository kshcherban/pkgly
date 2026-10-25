// ABOUTME: Tests MCP argument typing and CLI command conversion at the adapter boundary.
// ABOUTME: Protects defaults, literal positional values, upload fields, and JSON results.
#![allow(clippy::expect_used)]
use super::*;
use crate::cli::{Commands, PackageCommands, PackageUploadCommands};

fn parse(name: &str, arguments: Value) -> Result<crate::cli::Cli, String> {
    let actions = schema::actions();
    let action = actions
        .iter()
        .find(|action| action.tool.name == name)
        .expect("action exists");
    action.parse(arguments.as_object().expect("arguments"))
}

#[test]
fn search_preserves_defaults_and_treats_flag_like_queries_as_data() {
    let cli = parse("package_search", json!({"query":"--token=untrusted"})).expect("parse search");
    assert!(
        matches!(cli.command, Commands::Package(PackageCommands::Search { query, limit: 25 }) if query == "--token=untrusted")
    );
    assert!(cli.global.token.is_none());
    let cli = parse("package_search", json!({"query":"pkg", "limit":7})).expect("parse limit");
    assert!(matches!(
        cli.command,
        Commands::Package(PackageCommands::Search { limit: 7, .. })
    ));
}

#[test]
fn repository_json_config_does_not_replace_global_config_file_arguments() {
    let cli = parse(
        "repo_create",
        json!({"repository_type":"maven", "name":"releases", "storage":"local",
        "config":"{\"maven\":{\"type\":\"Hosted\"}}"}),
    )
    .expect("repository creation");
    assert!(cli.global.config.is_none());
    assert!(
        matches!(cli.command, Commands::Repo(crate::cli::RepoCommands::Create { config: Some(config), .. })
        if config == "{\"maven\":{\"type\":\"Hosted\"}}")
    );
}

#[test]
fn typed_arguments_reject_wrong_values_and_unknown_fields() {
    for arguments in [
        json!({"query":"pkg", "limit":-1}),
        json!({"query":"pkg", "limit":"25"}),
        json!({"query":"pkg", "limit":1.5}),
        json!({"query":"pkg", "token":"secret"}),
    ] {
        assert!(parse("package_search", arguments).is_err());
    }
    assert!(
        parse(
            "repo_delete",
            json!({"repository":"local/maven", "yes":"true"})
        )
        .is_err()
    );
    assert!(
        parse(
            "package_delete",
            json!({"repository":"local/maven", "paths":[1]})
        )
        .is_err()
    );
}

#[test]
fn go_upload_preserves_distinct_file_arguments() {
    let cli = parse(
        "package_upload_go",
        json!({"repository":"local/go", "module_name":"example.com/pkg",
        "version":"v1.0.0", "module":"module.zip", "info":"version.info", "gomod":"go.mod"}),
    )
    .expect("parse upload");
    assert!(
        matches!(cli.command, Commands::Package(PackageCommands::Upload(PackageUploadCommands::Go {
        module, info, gomod, module_name, version, ..
    })) if module.to_str() == Some("module.zip") && info.to_str() == Some("version.info")
        && gomod.to_str() == Some("go.mod") && module_name == "example.com/pkg" && version == "v1.0.0")
    );
}

#[test]
fn output_wraps_arrays_and_text_without_losing_json_values() {
    assert_eq!(
        decode_output(b"[{\"id\":1}]\n").expect("array"),
        json!({"items":[{"id":1}]})
    );
    assert_eq!(
        decode_output(b"{\"deleted\":2}\n").expect("object"),
        json!({"deleted":2})
    );
    assert_eq!(
        decode_output(b"token saved\n").expect("text"),
        json!({"message":"token saved"})
    );
    assert!(decode_output(&[0xff]).is_err());
}
