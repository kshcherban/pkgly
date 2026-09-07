use std::sync::Once;

use ahash::{HashMap, HashMapExt};
use serde::{Deserialize, Serialize};
use tracing_subscriber::{Layer, filter::Targets, layer::SubscriberExt, util::SubscriberInitExt};

use crate::logging::{LevelSerde, LoggingLevels};
#[cfg(test)]
mod tests;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestingLoggerConfig {
    pub levels: LoggingLevels,
}
impl TestingLoggerConfig {
    pub fn init(self) {
        static ONCE: Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let targets: Targets = self.levels.into();
            let stdout_log = tracing_subscriber::fmt::layer()
                .without_time()
                .with_thread_ids(false)
                .with_thread_names(false);
            tracing_subscriber::registry()
                .with(stdout_log.with_filter(targets))
                .init();
        });
    }
}
impl Default for TestingLoggerConfig {
    fn default() -> Self {
        // ponytail: default=Warn keeps third-party (aws_sdk, sqlx, hyper) spam out of test
        // output; raise per-crate levels in storage_testing_config.toml or RUST_LOG if needed.
        let mut others = HashMap::new();
        others.insert("pkgly".to_string(), LevelSerde::Debug);
        others.insert("nr_core".to_string(), LevelSerde::Debug);
        others.insert("nr_storage".to_string(), LevelSerde::Debug);
        Self {
            levels: LoggingLevels {
                default: LevelSerde::Warn,
                others,
            },
        }
    }
}
