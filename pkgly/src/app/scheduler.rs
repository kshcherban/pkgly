use chrono::Utc;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::app::Pkgly;

/// Background scheduler loop for periodic maintenance tasks.
///
/// This is intentionally generic so other repository lifecycle jobs can be added over time.
pub fn start_background_scheduler(site: Pkgly) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        // `interval` yields an immediate tick; consume it to delay the first run.
        interval.tick().await;
        loop {
            interval.tick().await;
            let now = Utc::now();

            match crate::repository::deb::scheduler::deb_proxy_scheduler_tick(site.clone(), now)
                .await
            {
                Ok(summary) => {
                    if summary.started > 0 || summary.skipped_running > 0 || summary.failed > 0 {
                        info!(?summary, "Background scheduler tick completed");
                    }
                }
                Err(err) => {
                    warn!(error = %err, "Background scheduler tick failed");
                }
            }
        }
    })
}
