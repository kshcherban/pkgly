// ABOUTME: Serves the existing pkglyctl actions as typed MCP tools over stdio.
// ABOUTME: Captures CLI output, reloads profiles, and serializes credential mutations.
mod schema;

use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, JsonObject,
        ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext,
};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::{
    CliError,
    cli::{GlobalArgs, OutputMode},
    config::{self, ConfigFile, ConfigOverrides, EnvConfig},
};

struct McpServer {
    global: GlobalArgs,
    env: EnvConfig,
    actions: Vec<schema::Action>,
    execution: Mutex<()>,
}

pub(super) async fn serve(global: GlobalArgs, env: EnvConfig) -> Result<i32, CliError> {
    let server = McpServer {
        global,
        env,
        actions: schema::actions(),
        execution: Mutex::new(()),
    };
    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|_| CliError::Message("MCP initialization failed".to_string()))?;
    service
        .waiting()
        .await
        .map_err(|_| CliError::Message("MCP transport failed".to_string()))?;
    Ok(0)
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("pkglyctl", env!("CARGO_PKG_VERSION")))
            .with_instructions("Pkgly CLI actions. Credentials follow CLI flag/environment/profile precedence. Downloads require a local output_file. Deletions require yes or force. Repository scopes are enforced by Pkgly; management APIs retain their role checks.")
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.actions
            .iter()
            .find(|action| action.tool.name == name)
            .map(|action| action.tool.clone())
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        if request.and_then(|params| params.cursor).is_some() {
            return Err(ErrorData::invalid_params("invalid tools cursor", None));
        }
        Ok(ListToolsResult::with_all_items(
            self.actions
                .iter()
                .map(|action| action.tool.clone())
                .collect(),
        ))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let action = self
            .actions
            .iter()
            .find(|action| action.tool.name == request.name)
            .ok_or_else(|| ErrorData::invalid_params("unknown Pkgly tool", None))?;
        let values = request.arguments.unwrap_or_default();
        let _guard = self.execution.lock().await;
        let secrets = self.secrets(&values);
        let result = match self.execute(action, &values).await {
            Ok(value) => CallToolResult::structured(value),
            Err(message) => {
                let message = secrets
                    .iter()
                    .filter(|secret| !secret.is_empty())
                    .fold(message, |message, secret| message.replace(secret, "***"));
                CallToolResult::structured_error(json!({"message": message}))
            }
        };
        Ok(result.into())
    }
}

impl McpServer {
    async fn execute(&self, action: &schema::Action, values: &JsonObject) -> Result<Value, String> {
        let mut cli = action.parse(values)?;
        // Global configuration belongs to the launched process, rather than a tool's arguments.
        cli.global = self.global.clone();
        cli.global.output = Some(OutputMode::Json);
        let mut output = Vec::new();
        crate::run_with_password_prompt(cli, self.env.clone(), &mut output, || {
            Err(CliError::Message(
                "MCP login requires a password argument".to_string(),
            ))
        })
        .await
        .map_err(|error| match error {
            CliError::Config(config::ConfigError::TomlDe(_)) => {
                "invalid CLI configuration (TOML)".to_string()
            }
            other => other.to_string(),
        })?;
        if action.tool.name == "package_download" {
            let path = values
                .get("output_file")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing output_file".to_string())?;
            let metadata = tokio::fs::metadata(path)
                .await
                .map_err(|error| error.to_string())?;
            return Ok(json!({"path": path, "bytes": metadata.len()}));
        }
        decode_output(&output)
    }

    fn secrets(&self, arguments: &JsonObject) -> Vec<String> {
        let mut secrets: Vec<_> = self
            .global
            .token
            .iter()
            .chain(self.env.token.iter())
            .cloned()
            .collect();
        secrets.extend(["password", "token"].into_iter().filter_map(|key| {
            arguments
                .get(key)
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        }));
        let path = config::config_path(&ConfigOverrides::from_global(&self.global), &self.env);
        if let Ok(config) = ConfigFile::load_or_default(&path) {
            secrets.extend(
                config
                    .profiles
                    .into_values()
                    .filter_map(|profile| profile.token),
            );
        }
        secrets
    }
}

fn decode_output(output: &[u8]) -> Result<Value, String> {
    let text =
        std::str::from_utf8(output).map_err(|_| "command returned non-text output".to_string())?;
    match serde_json::from_str(text) {
        Ok(value @ Value::Object(_)) => Ok(value),
        Ok(Value::Array(items)) => Ok(json!({"items": items})),
        _ => Ok(json!({"message": text.trim_end()})),
    }
}

#[cfg(test)]
mod tests;
