pub mod config;

use config::{
    AppLogger, AppLoggerType, ConsoleLogFormat, ConsoleLogger, LoggingConfig, MetricsConfig,
    OtelConfig, RollingFileLogger,
};
use nr_core::logging::LoggingLevels;
use opentelemetry::{global, trace::TracerProvider as _};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, MetricExporter, SpanExporter, WithExportConfig};
use opentelemetry_sdk::{
    Resource,
    logs::SdkLoggerProvider,
    metrics::{PeriodicReader, SdkMeterProvider},
    propagation::TraceContextPropagator,
    trace::SdkTracerProvider,
};
use tracing_appender::rolling::RollingFileAppender;
use tracing_subscriber::{
    Layer, Registry, filter::Targets, layer::SubscriberExt, util::SubscriberInitExt,
};

struct TracerResult {
    levels: LoggingLevels,
    logging: Option<SdkLoggerProvider>,
    tracing: Option<SdkTracerProvider>,
}
fn tracer(config: OtelConfig) -> anyhow::Result<Option<TracerResult>> {
    if !config.enabled {
        return Ok(None);
    }
    let resources: Resource = config.config.into();

    let tracer = if config.traces {
        let exporter = SpanExporter::builder()
            .with_tonic()
            .with_protocol(config.protocol.into())
            .with_endpoint(&config.endpoint);
        let provider = SdkTracerProvider::builder()
            .with_resource(resources.clone())
            .with_batch_exporter(exporter.build()?)
            .build();
        Some(provider)
    } else {
        None
    };
    let logger = if config.logs {
        let exporter = LogExporter::builder()
            .with_tonic()
            .with_protocol(config.protocol.into())
            .with_endpoint(&config.endpoint);
        let provider = SdkLoggerProvider::builder()
            .with_resource(resources.clone())
            .with_batch_exporter(exporter.build()?)
            .build();
        Some(provider)
    } else {
        None
    };

    Ok(Some(TracerResult {
        levels: config.levels,
        logging: logger,
        tracing: tracer,
    }))
}

fn metrics(config: MetricsConfig) -> anyhow::Result<SdkMeterProvider> {
    let resources: Resource = config.config.into();

    let exporter = MetricExporter::builder()
        .with_tonic()
        .with_protocol(config.protocol.into())
        .with_endpoint(&config.endpoint)
        .build()?;
    let reader = PeriodicReader::builder(exporter).build();

    Ok(SdkMeterProvider::builder()
        .with_reader(reader)
        .with_resource(resources)
        .build())
}

pub fn init(log_config: LoggingConfig, otel_config: OtelConfig) -> anyhow::Result<LoggingState> {
    let mut layers: Vec<Box<dyn Layer<Registry> + Send + Sync>> =
        Vec::with_capacity(log_config.loggers.len() + 1); // +1 for potential OTEL
    let mut state = LoggingState {
        items: Vec::with_capacity(log_config.loggers.len() + 1),
        ..Default::default()
    };
    let LoggingConfig {
        loggers,
        metrics: metrics_config,
        levels: parent_levels,
    } = log_config;

    for (name, logger) in loggers.into_iter().map(|(k, mut v)| {
        v.get_levels_mut().inherit_from(&parent_levels);
        (k, v)
    }) {
        match logger {
            AppLogger::Otel(config) => {
                let Some(TracerResult {
                    mut levels,
                    logging,
                    tracing,
                }) = tracer(config)?
                else {
                    continue;
                };
                state.set_global_text_propagator();

                levels.inherit_from(&parent_levels);
                let logging_levels: Targets = levels.into();
                if let Some(tracer_provider) = tracing {
                    let tracer = tracer_provider.tracer(name.clone());
                    state.items.push(NamedLogger {
                        name: name.clone(),
                        logger: LoggingStateItem::Tracer(tracer_provider),
                    });
                    let otel_layer = tracing_subscriber::Layer::with_filter(
                        tracing_opentelemetry::layer().with_tracer(tracer).boxed(),
                        logging_levels.clone(),
                    );
                    layers.push(otel_layer.boxed());
                }
                if let Some(logging_provider) = logging {
                    let tracing_bridge = OpenTelemetryTracingBridge::new(&logging_provider);
                    state.items.push(NamedLogger {
                        name: name.clone(),
                        logger: LoggingStateItem::Logger(logging_provider),
                    });

                    let otel_layer =
                        tracing_subscriber::Layer::with_filter(tracing_bridge, logging_levels);

                    layers.push(otel_layer.boxed());
                }
            }
            AppLogger::Console(config) => {
                let ConsoleLogger {
                    format,
                    pretty,
                    levels,
                    rules,
                } = config;
                let logging_levels: Targets = levels.into();
                let format = format.unwrap_or_else(|| {
                    if pretty.unwrap_or(false) {
                        ConsoleLogFormat::Pretty
                    } else {
                        ConsoleLogFormat::Full
                    }
                });
                layers.push(rules.fmt_layer_for_registry(format, logging_levels));
            }
            AppLogger::RollingFile(config) => {
                let RollingFileLogger {
                    levels,
                    rules,
                    path,
                    file_prefix,
                    interval,
                } = config;
                let logging_levels: Targets = levels.into();

                let file_appender =
                    RollingFileAppender::new(interval.into(), path.clone(), file_prefix.clone());
                layers.push(rules.fmt_layer_for_registry_with_writer(
                    ConsoleLogFormat::Full,
                    logging_levels,
                    file_appender,
                ));
            }
        }
    }

    // Handle standalone OpenTelemetry configuration
    if otel_config.enabled {
        let Some(TracerResult {
            mut levels,
            logging,
            tracing,
        }) = tracer(otel_config)?
        else {
            // This shouldn't happen since we checked enabled, but just in case
            let subscriber = Registry::default().with(layers);
            subscriber.init();
            return Ok(state);
        };

        state.set_global_text_propagator();
        levels.inherit_from(&parent_levels);
        let logging_levels: Targets = levels.into();

        if let Some(tracer_provider) = tracing {
            let tracer = tracer_provider.tracer("opentelemetry");
            state.items.push(NamedLogger {
                name: "opentelemetry".to_string(),
                logger: LoggingStateItem::Tracer(tracer_provider),
            });
            let otel_layer = tracing_subscriber::Layer::with_filter(
                tracing_opentelemetry::layer().with_tracer(tracer).boxed(),
                logging_levels.clone(),
            );
            layers.push(otel_layer.boxed());
        }

        if let Some(logging_provider) = logging {
            let tracing_bridge = OpenTelemetryTracingBridge::new(&logging_provider);
            state.items.push(NamedLogger {
                name: "opentelemetry-logs".to_string(),
                logger: LoggingStateItem::Logger(logging_provider),
            });

            let otel_layer = tracing_subscriber::Layer::with_filter(tracing_bridge, logging_levels);

            layers.push(otel_layer.boxed());
        }
    }

    let subscriber = Registry::default().with(layers);
    subscriber.init();
    if let Some(metrics_config) = metrics_config
        && metrics_config.enabled
    {
        let provider = metrics(metrics_config)?;
        global::set_meter_provider(provider.clone());
        state.items.push(NamedLogger {
            name: "metrics".to_string(),
            logger: LoggingStateItem::Meter(provider),
        });
    }
    Ok(state)
}

#[derive(Debug, Default)]
pub struct LoggingState {
    pub items: Vec<NamedLogger>,
    has_set_global_text_propagator: bool,
}
impl LoggingState {
    pub fn close(self) -> anyhow::Result<()> {
        for item in self.items {
            let NamedLogger { logger, name } = item;
            println!("Shutting down logger: {} {:?}", name, logger);
            match logger {
                LoggingStateItem::Logger(logger) => logger.shutdown()?,
                LoggingStateItem::Tracer(tracer) => tracer.shutdown()?,
                LoggingStateItem::Meter(meter) => meter.shutdown()?,
            }
        }

        Ok(())
    }

    fn set_global_text_propagator(&mut self) {
        if self.has_set_global_text_propagator {
            return;
        }
        global::set_text_map_propagator(TraceContextPropagator::new());
        self.has_set_global_text_propagator = true;
    }
}
#[derive(Debug)]
pub enum LoggingStateItem {
    Logger(SdkLoggerProvider),
    Tracer(SdkTracerProvider),
    Meter(SdkMeterProvider),
}
#[derive(Debug)]
pub struct NamedLogger {
    pub name: String,
    pub logger: LoggingStateItem,
}
