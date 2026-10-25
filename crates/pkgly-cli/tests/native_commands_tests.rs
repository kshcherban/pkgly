// ABOUTME: Verifies package manager commands target the registry's public endpoints.
// ABOUTME: Protects the sparse Cargo index URL advertised by CLI and MCP tools.
use pkgly_cli::{cli::NativeCommands, config::ResolvedConfig, native};

#[test]
fn cargo_publish_uses_the_sparse_index_endpoint() -> Result<(), Box<dyn std::error::Error>> {
    let config = ResolvedConfig {
        base_url: "http://localhost:8000/".into(),
        token: None,
        profile: None,
        default_storage: None,
    };
    let command = native::render(
        NativeCommands::Cargo {
            repository: "local/cargo".into(),
        },
        &config,
    )?;
    assert_eq!(
        command,
        "cargo publish --registry pkgly --index sparse+http://localhost:8000/repositories/local/cargo/index/"
    );
    Ok(())
}

#[test]
fn docker_push_uses_the_registry_storage_repository_prefix()
-> Result<(), Box<dyn std::error::Error>> {
    let config = ResolvedConfig {
        base_url: "http://localhost:8000/".into(),
        ..Default::default()
    };
    let command = native::render(
        NativeCommands::Docker {
            repository: "local/docker".into(),
            image: Some("app:0.1.0".into()),
        },
        &config,
    )?;
    assert_eq!(
        command,
        "docker login localhost:8000 && docker tag app:0.1.0 localhost:8000/local/docker/app:0.1.0 && docker push localhost:8000/local/docker/app:0.1.0"
    );
    Ok(())
}
