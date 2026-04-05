mod otel;
use std::fmt;
use std::path::PathBuf;

use ahash::{HashMap, HashMapExt};
use nr_core::logging::{LevelSerde, LoggingLevels};
pub use otel::*;
use serde::{Deserialize, Serialize};
use tracing_appender::rolling::Rotation;
use tracing_subscriber::{Layer, Registry, filter::Targets};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub loggers: HashMap<String, AppLogger>,
    pub metrics: Option<MetricsConfig>,
    pub levels: LoggingLevels,
}
impl Default for LoggingConfig {
    fn default() -> Self {
        let mut loggers = HashMap::new();
        loggers.insert(
            "console".to_string(),
            AppLogger::Console(ConsoleLogger::default()),
        );
        loggers.insert(
            "file".to_string(),
            AppLogger::RollingFile(RollingFileLogger::default()),
        );
        Self {
            loggers,
            metrics: Some(MetricsConfig::default()),
            levels: default_log_levels(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "config")]
pub enum AppLogger {
    Otel(OtelConfig),
    Console(ConsoleLogger),
    RollingFile(RollingFileLogger),
}
pub trait AppLoggerType {
    fn get_levels_mut(&mut self) -> &mut LoggingLevels;
}
impl AppLoggerType for AppLogger {
    fn get_levels_mut(&mut self) -> &mut LoggingLevels {
        match self {
            AppLogger::Otel(config) => &mut config.levels,
            AppLogger::Console(config) => &mut config.levels,
            AppLogger::RollingFile(config) => &mut config.levels,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct StandardLoggerFmtRules {
    pub include_time: bool,
    pub include_level: bool,
    pub include_line_numbers: bool,
    pub include_file: bool,
    pub include_target: bool,
    pub include_span_context: bool,
    pub ansi_color: bool,
    pub include_thread_ids: bool,
    pub include_thread_names: bool,
}
impl Default for StandardLoggerFmtRules {
    fn default() -> Self {
        Self {
            include_time: true,
            include_level: true,
            include_line_numbers: false,
            include_file: false,
            include_target: true,
            include_span_context: true,
            ansi_color: true,
            include_thread_ids: false,
            include_thread_names: false,
        }
    }
}
impl StandardLoggerFmtRules {
    pub(crate) fn fmt_layer_for_registry(
        &self,
        format: ConsoleLogFormat,
        filter: Targets,
    ) -> Box<dyn Layer<Registry> + Send + Sync> {
        match format {
            ConsoleLogFormat::Compact => {
                let layer = tracing_subscriber::fmt::layer::<Registry>()
                    // In compact mode, only the level is colorized (when enabled) by our custom
                    // formatter. Keep the `fmt` layer itself non-ANSI to avoid styling fields
                    // (italics/dim/background) in docker logs.
                    .with_ansi(false)
                    .event_format(CompactTextEventFormat { rules: *self });
                layer.with_filter(filter).boxed()
            }
            ConsoleLogFormat::Json => {
                let layer = tracing_subscriber::fmt::layer::<Registry>()
                    .with_ansi(self.ansi_color)
                    .with_target(self.include_target)
                    .with_line_number(self.include_line_numbers)
                    .with_file(self.include_file)
                    .with_level(self.include_level)
                    .with_thread_ids(self.include_thread_ids)
                    .with_thread_names(self.include_thread_names)
                    .json()
                    .with_current_span(self.include_span_context)
                    .with_span_list(self.include_span_context);
                if self.include_time {
                    layer.with_filter(filter).boxed()
                } else {
                    layer.without_time().with_filter(filter).boxed()
                }
            }
            ConsoleLogFormat::Pretty => {
                let layer = tracing_subscriber::fmt::layer::<Registry>()
                    .with_ansi(self.ansi_color)
                    .with_target(self.include_target)
                    .with_line_number(self.include_line_numbers)
                    .with_file(self.include_file)
                    .with_level(self.include_level)
                    .with_thread_ids(self.include_thread_ids)
                    .with_thread_names(self.include_thread_names)
                    .pretty();
                if self.include_time {
                    layer.with_filter(filter).boxed()
                } else {
                    layer.without_time().with_filter(filter).boxed()
                }
            }
            ConsoleLogFormat::Full => {
                let layer = tracing_subscriber::fmt::layer::<Registry>()
                    .with_ansi(self.ansi_color)
                    .with_target(self.include_target)
                    .with_line_number(self.include_line_numbers)
                    .with_file(self.include_file)
                    .with_level(self.include_level)
                    .with_thread_ids(self.include_thread_ids)
                    .with_thread_names(self.include_thread_names);
                if self.include_time {
                    layer.with_filter(filter).boxed()
                } else {
                    layer.without_time().with_filter(filter).boxed()
                }
            }
        }
    }

    pub(crate) fn fmt_layer_for_registry_with_writer<W>(
        &self,
        format: ConsoleLogFormat,
        filter: Targets,
        writer: W,
    ) -> Box<dyn Layer<Registry> + Send + Sync>
    where
        W: for<'a> tracing_subscriber::fmt::MakeWriter<'a> + Send + Sync + 'static,
    {
        match format {
            ConsoleLogFormat::Compact => {
                let layer = tracing_subscriber::fmt::layer::<Registry>()
                    .with_writer(writer)
                    // In compact mode, only the level is colorized (when enabled) by our custom
                    // formatter. Keep the `fmt` layer itself non-ANSI to avoid styling fields
                    // (italics/dim/background) in docker logs.
                    .with_ansi(false)
                    .event_format(CompactTextEventFormat { rules: *self });
                layer.with_filter(filter).boxed()
            }
            ConsoleLogFormat::Json => {
                let layer = tracing_subscriber::fmt::layer::<Registry>()
                    .with_writer(writer)
                    .with_ansi(self.ansi_color)
                    .with_target(self.include_target)
                    .with_line_number(self.include_line_numbers)
                    .with_file(self.include_file)
                    .with_level(self.include_level)
                    .with_thread_ids(self.include_thread_ids)
                    .with_thread_names(self.include_thread_names)
                    .json()
                    .with_current_span(self.include_span_context)
                    .with_span_list(self.include_span_context);
                if self.include_time {
                    layer.with_filter(filter).boxed()
                } else {
                    layer.without_time().with_filter(filter).boxed()
                }
            }
            ConsoleLogFormat::Pretty => {
                let layer = tracing_subscriber::fmt::layer::<Registry>()
                    .with_writer(writer)
                    .with_ansi(self.ansi_color)
                    .with_target(self.include_target)
                    .with_line_number(self.include_line_numbers)
                    .with_file(self.include_file)
                    .with_level(self.include_level)
                    .with_thread_ids(self.include_thread_ids)
                    .with_thread_names(self.include_thread_names)
                    .pretty();
                if self.include_time {
                    layer.with_filter(filter).boxed()
                } else {
                    layer.without_time().with_filter(filter).boxed()
                }
            }
            ConsoleLogFormat::Full => {
                let layer = tracing_subscriber::fmt::layer::<Registry>()
                    .with_writer(writer)
                    .with_ansi(self.ansi_color)
                    .with_target(self.include_target)
                    .with_line_number(self.include_line_numbers)
                    .with_file(self.include_file)
                    .with_level(self.include_level)
                    .with_thread_ids(self.include_thread_ids)
                    .with_thread_names(self.include_thread_names);
                if self.include_time {
                    layer.with_filter(filter).boxed()
                } else {
                    layer.without_time().with_filter(filter).boxed()
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsoleLogFormat {
    Compact,
    Pretty,
    Json,
    Full,
}
impl Default for ConsoleLogFormat {
    fn default() -> Self {
        Self::Full
    }
}

#[derive(Debug, Clone, Copy)]
struct CompactTextEventFormat {
    rules: StandardLoggerFmtRules,
}

impl<S, N> tracing_subscriber::fmt::format::FormatEvent<S, N> for CompactTextEventFormat
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
    N: for<'a> tracing_subscriber::fmt::format::FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &tracing_subscriber::fmt::FmtContext<'_, S, N>,
        mut writer: tracing_subscriber::fmt::format::Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> fmt::Result {
        use tracing_subscriber::fmt::FormatFields as _;
        use tracing_subscriber::fmt::time::FormatTime as _;

        let meta = event.metadata();

        if self.rules.include_time {
            let timer = tracing_subscriber::fmt::time::SystemTime;
            timer.format_time(&mut writer)?;
            writer.write_char(' ')?;
        }

        if self.rules.include_level {
            if self.rules.ansi_color {
                let (prefix, suffix) = match *meta.level() {
                    tracing::Level::ERROR => ("\x1b[31m", "\x1b[0m"),
                    tracing::Level::WARN => ("\x1b[33m", "\x1b[0m"),
                    tracing::Level::INFO => ("\x1b[32m", "\x1b[0m"),
                    tracing::Level::DEBUG => ("\x1b[34m", "\x1b[0m"),
                    tracing::Level::TRACE => ("\x1b[35m", "\x1b[0m"),
                };
                write!(writer, "{prefix}{}{suffix} ", meta.level())?;
            } else {
                write!(writer, "{} ", meta.level())?;
            }
        }

        if self.rules.include_thread_names {
            if let Some(name) = std::thread::current().name() {
                write!(writer, "{} ", name)?;
            }
        }

        if self.rules.include_thread_ids {
            write!(writer, "{:?} ", std::thread::current().id())?;
        }

        if self.rules.include_span_context {
            if let Some(scope) = ctx.event_scope() {
                let mut wrote_any = false;
                for span in scope.from_root() {
                    if !wrote_any {
                        writer.write_str("[")?;
                        wrote_any = true;
                    } else {
                        writer.write_str("::")?;
                    }
                    writer.write_str(span.name())?;
                }
                if wrote_any {
                    writer.write_str("] ")?;
                }
            }
        }

        if self.rules.include_target {
            writer.write_str(meta.target())?;
            writer.write_str(": ")?;
        }

        if self.rules.include_file {
            if let Some(file) = meta.file() {
                writer.write_str(file)?;
                if self.rules.include_line_numbers {
                    if let Some(line) = meta.line() {
                        write!(writer, ":{line}")?;
                    }
                }
                writer.write_str(": ")?;
            }
        }

        ctx.format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ConsoleLogger {
    pub format: Option<ConsoleLogFormat>,
    pub pretty: Option<bool>,
    #[serde(flatten)]
    pub rules: StandardLoggerFmtRules,
    pub levels: LoggingLevels,
}
impl AppLoggerType for ConsoleLogger {
    fn get_levels_mut(&mut self) -> &mut LoggingLevels {
        &mut self.levels
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollingFileLogger {
    pub path: PathBuf,
    pub file_prefix: String,
    pub levels: LoggingLevels,

    pub interval: RollingInterval,
    #[serde(flatten)]
    pub rules: StandardLoggerFmtRules,
}
impl AppLoggerType for RollingFileLogger {
    fn get_levels_mut(&mut self) -> &mut LoggingLevels {
        &mut self.levels
    }
}
impl Default for RollingFileLogger {
    fn default() -> Self {
        Self {
            path: PathBuf::from("logs/app.log"),
            file_prefix: "thd-helper.log".to_string(),
            levels: LoggingLevels::default(),
            interval: RollingInterval::Daily,
            rules: StandardLoggerFmtRules::default(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RollingInterval {
    Minutely,
    Hourly,
    Daily,
    Never,
}

impl From<RollingInterval> for Rotation {
    fn from(value: RollingInterval) -> Self {
        match value {
            RollingInterval::Minutely => Rotation::MINUTELY,
            RollingInterval::Hourly => Rotation::HOURLY,
            RollingInterval::Daily => Rotation::DAILY,
            RollingInterval::Never => Rotation::NEVER,
        }
    }
}
pub fn default_log_levels() -> LoggingLevels {
    let mut others = HashMap::new();
    others.insert("pkgly".to_string(), LevelSerde::Debug);
    others.insert("nr_core".to_string(), LevelSerde::Debug);
    others.insert("nr_storage".to_string(), LevelSerde::Debug);
    others.insert("sqlx".to_owned(), LevelSerde::Debug);
    others.insert("pg_extended_sqlx_queries".to_owned(), LevelSerde::Debug);
    others.insert("h2".to_string(), LevelSerde::Warn);
    others.insert("tower".to_string(), LevelSerde::Warn);
    others.insert("tonic".to_string(), LevelSerde::Warn);
    others.insert("hyper_util".to_string(), LevelSerde::Warn);

    LoggingLevels {
        default: LevelSerde::Info,
        others,
    }
}

#[cfg(test)]
mod tests;
