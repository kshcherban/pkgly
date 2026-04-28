use serde_json::Value;

use crate::cli::OutputMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Table,
    Json,
}

impl From<OutputMode> for OutputFormat {
    fn from(value: OutputMode) -> Self {
        match value {
            OutputMode::Table => Self::Table,
            OutputMode::Json => Self::Json,
        }
    }
}

impl OutputFormat {
    pub fn render_value(&self, value: &Value) -> Result<String, serde_json::Error> {
        match self {
            Self::Json => render_json(value),
            Self::Table => Ok(render_value_table(value)),
        }
    }

    pub fn render_rows(&self, headers: &[&str], rows: &[Vec<String>]) -> String {
        match self {
            Self::Json => {
                let values = rows
                    .iter()
                    .map(|row| {
                        let mut object = serde_json::Map::new();
                        for (index, header) in headers.iter().enumerate() {
                            object.insert(
                                header.to_ascii_lowercase().replace(' ', "_"),
                                Value::String(row.get(index).cloned().unwrap_or_default()),
                            );
                        }
                        Value::Object(object)
                    })
                    .collect::<Vec<_>>();
                render_json(&Value::Array(values)).unwrap_or_else(|_| "[]\n".to_string())
            }
            Self::Table => render_table(headers, rows),
        }
    }
}

pub fn render_json_pretty(value: &Value) -> Result<String, serde_json::Error> {
    let mut output = serde_json::to_string_pretty(value)?;
    output.push('\n');
    Ok(output)
}

fn render_json(value: &Value) -> Result<String, serde_json::Error> {
    let mut output = serde_json::to_string(value)?;
    output.push('\n');
    Ok(output)
}

fn render_value_table(value: &Value) -> String {
    match value {
        Value::Array(items) => {
            let rows = items.iter().map(summary_row).collect::<Vec<Vec<String>>>();
            render_table(&["Value"], &rows)
        }
        Value::Object(object) => {
            let rows = object
                .iter()
                .map(|(key, value)| vec![key.clone(), display_value(value)])
                .collect::<Vec<_>>();
            render_table(&["Key", "Value"], &rows)
        }
        other => {
            let mut output = display_value(other);
            output.push('\n');
            output
        }
    }
}

fn summary_row(value: &Value) -> Vec<String> {
    match value {
        Value::Object(object) => {
            if let Some(name) = object.get("name").or_else(|| object.get("repository_name")) {
                vec![display_value(name)]
            } else {
                vec![display_value(value)]
            }
        }
        other => vec![display_value(other)],
    }
}

fn display_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths = headers.iter().map(|value| value.len()).collect::<Vec<_>>();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if let Some(width) = widths.get_mut(index) {
                *width = (*width).max(cell.len());
            }
        }
    }

    let mut output = String::new();
    output.push_str(&format_row(
        &headers
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>(),
        &widths,
    ));
    for row in rows {
        output.push_str(&format_row(row, &widths));
    }
    output
}

fn format_row(row: &[String], widths: &[usize]) -> String {
    let mut output = String::new();
    for (index, width) in widths.iter().enumerate() {
        if index > 0 {
            output.push_str("  ");
        }
        let cell = row.get(index).map(String::as_str).unwrap_or("");
        output.push_str(cell);
        if index + 1 < widths.len() {
            for _ in cell.len()..*width {
                output.push(' ');
            }
        }
    }
    output.push('\n');
    output
}

pub fn redact_secret(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    if chars.len() <= 8 {
        return "***".to_string();
    }
    let prefix: String = chars.iter().take(4).collect();
    let suffix: String = chars[chars.len().saturating_sub(4)..].iter().collect();
    format!("{prefix}...{suffix}")
}
