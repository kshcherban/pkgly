// ABOUTME: Derives MCP tool definitions and typed arguments from the CLI command tree.
// ABOUTME: Converts JSON inputs into clap commands without invoking a shell.
use std::any::TypeId;

use clap::{Arg, ArgAction, Command, CommandFactory, FromArgMatches};
use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::{Value, json};

use crate::cli::{Cli, Commands, GlobalArgs};

pub(super) struct Action {
    pub tool: Tool,
    path: Vec<String>,
    arguments: Vec<Arg>,
}

pub(super) fn actions() -> Vec<Action> {
    let mut command = Cli::command();
    command.build();
    let mut actions = Vec::new();
    collect_actions(&command, &[], &mut actions);
    actions
}

fn collect_actions(command: &Command, path: &[String], actions: &mut Vec<Action>) {
    if command.has_subcommands() {
        for child in command.get_subcommands() {
            if matches!(child.get_name(), "mcp" | "help") {
                continue;
            }
            let mut child_path = path.to_vec();
            child_path.push(child.get_name().to_string());
            collect_actions(child, &child_path, actions);
        }
        return;
    }
    actions.push(Action::new(command, path.to_vec()));
}

impl Action {
    fn new(command: &Command, path: Vec<String>) -> Self {
        let name = path.join("_").replace('-', "_");
        let arguments: Vec<_> = command
            .get_arguments()
            .filter(|arg| {
                !arg.is_global_set()
                    && !matches!(arg.get_action(), ArgAction::Help | ArgAction::Version)
            })
            .cloned()
            .collect();
        let description = command
            .get_about()
            .map(ToString::to_string)
            .unwrap_or_default();
        let schema = input_schema(&arguments, &name);
        let mut tool = Tool::new(name, description, schema);
        tool.annotations = Some(annotations(&path));
        Self {
            tool,
            path,
            arguments,
        }
    }

    pub fn parse(&self, values: &JsonObject) -> Result<Cli, String> {
        let mut argv = vec!["pkglyctl".to_string()];
        argv.extend(self.path.iter().cloned());
        let mut positional = Vec::new();
        for key in values.keys() {
            if !self
                .arguments
                .iter()
                .any(|arg| arg.get_id().as_str() == key)
            {
                return Err(format!("unknown argument `{key}`"));
            }
        }
        for arg in &self.arguments {
            self.append_argument(arg, values, &mut argv, &mut positional)?;
        }
        argv.push("--".to_string());
        argv.extend(positional);
        let matches = Cli::command()
            .try_get_matches_from(argv)
            .map_err(|err| err.to_string())?;
        // Decode only command fields; startup configuration is supplied by the MCP process.
        let command = Commands::from_arg_matches(&matches).map_err(|err| err.to_string())?;
        Ok(Cli {
            global: GlobalArgs::default(),
            command,
        })
    }

    fn append_argument(
        &self,
        arg: &Arg,
        values: &JsonObject,
        argv: &mut Vec<String>,
        positional: &mut Vec<String>,
    ) -> Result<(), String> {
        let key = arg.get_id().as_str();
        let required = self.tool.input_schema["required"]
            .as_array()
            .is_some_and(|keys| keys.iter().any(|entry| entry == key));
        let Some(value) = values.get(key).filter(|value| !value.is_null()) else {
            return if required {
                Err(format!("missing required argument `{key}`"))
            } else {
                Ok(())
            };
        };
        let schema = &self.tool.input_schema["properties"][key];
        let converted = argument_values(key, value, schema)?;
        if let Some(long) = arg.get_long() {
            if matches!(arg.get_action(), ArgAction::SetTrue) {
                if value == true {
                    argv.push(format!("--{long}"));
                }
            } else {
                argv.extend(
                    converted
                        .into_iter()
                        .map(|value| format!("--{long}={value}")),
                );
            }
        } else {
            positional.extend(converted);
        }
        Ok(())
    }
}

fn input_schema(arguments: &[Arg], name: &str) -> JsonObject {
    let mut properties = JsonObject::new();
    let mut required = Vec::new();
    for arg in arguments {
        let key = arg.get_id().as_str();
        properties.insert(key.to_string(), argument_schema(arg));
        if arg.is_required_set()
            || (name == "auth_login" && key == "password")
            || (name == "package_download" && key == "output_file")
        {
            required.push(key);
        }
    }
    let mut schema = JsonObject::new();
    schema.insert("type".into(), json!("object"));
    schema.insert("properties".into(), json!(properties));
    schema.insert("required".into(), json!(required));
    schema.insert("additionalProperties".into(), json!(false));
    schema
}

fn annotations(path: &[String]) -> ToolAnnotations {
    let read_only = !path.iter().any(|part| {
        matches!(
            part.as_str(),
            "login"
                | "logout"
                | "set-token"
                | "use"
                | "remove"
                | "create"
                | "delete"
                | "config-set"
                | "upload"
                | "download"
        )
    });
    ToolAnnotations::new()
        .read_only(read_only)
        .destructive(!read_only && !path.iter().any(|part| part == "create"))
}

fn argument_schema(arg: &Arg) -> Value {
    let value_type = arg.get_value_parser().type_id();
    let mut schema = if matches!(arg.get_action(), ArgAction::SetTrue) {
        json!({"type":"boolean"})
    } else if value_type == TypeId::of::<usize>() {
        json!({"type":"integer", "minimum":0})
    } else if arg
        .get_num_args()
        .is_some_and(|range| range.max_values() > 1)
    {
        json!({"type":"array", "items":{"type":"string"}})
    } else {
        json!({"type":"string"})
    };
    if let Some(help) = arg.get_help() {
        schema["description"] = json!(help.to_string());
    }
    if let Some(default) = arg
        .get_default_values()
        .first()
        .and_then(|value| value.to_str())
    {
        schema["default"] = match schema["type"].as_str() {
            Some("integer" | "boolean") => serde_json::from_str(default).unwrap_or(json!(default)),
            _ => json!(default),
        };
    }
    schema
}

fn argument_values(key: &str, value: &Value, schema: &Value) -> Result<Vec<String>, String> {
    let valid = match schema["type"].as_str() {
        Some("boolean") => value.is_boolean(),
        Some("integer") => value.as_u64().is_some(),
        Some("array") => value
            .as_array()
            .is_some_and(|items| items.iter().all(Value::is_string)),
        _ => value.is_string(),
    };
    if !valid {
        return Err(format!("`{key}` must have type {}", schema["type"]));
    }
    if let Some(items) = value.as_array() {
        return Ok(items
            .iter()
            .filter_map(|item| item.as_str().map(ToOwned::to_owned))
            .collect());
    }
    Ok(vec![
        value
            .as_str()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| value.to_string()),
    ])
}
