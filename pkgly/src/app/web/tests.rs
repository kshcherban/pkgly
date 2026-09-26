// ABOUTME: Verifies server startup helpers and web runtime configuration.
// ABOUTME: Covers default worker counts, explicit overrides, and zero-value fallback.
use super::resolve_worker_threads;
use crate::app::config::WebServer;

#[test]
fn default_uses_available_parallelism() {
    let web_server = WebServer::default();
    let expected = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1);
    assert_eq!(resolve_worker_threads(&web_server), expected);
}

#[test]
fn override_worker_threads_is_honored() {
    let mut web_server = WebServer::default();
    web_server.worker_threads = Some(3);
    assert_eq!(resolve_worker_threads(&web_server), 3);
}

#[test]
fn zero_worker_threads_falls_back_to_one() {
    let mut web_server = WebServer::default();
    web_server.worker_threads = Some(0);
    assert_eq!(resolve_worker_threads(&web_server), 1);
}
