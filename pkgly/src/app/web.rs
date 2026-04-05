use std::{
    fs::File,
    io::BufReader,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use axum::{Router, extract::Request};
use futures_util::pin_mut;
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use num_cpus;
use rustls::ServerConfig;
use rustls_pemfile::{certs, pkcs8_private_keys};
use tokio::{net::TcpListener, signal};
use tokio_rustls::TlsAcceptor;
use tower_service::Service;
use tracing::{debug, error, info, warn};

use super::Pkgly;
use super::routes;
use crate::app::config::WebServer;
use crate::config::{PkglyConfig, load_config};
/// Decide how many Tokio worker threads to start.
pub(crate) fn resolve_worker_threads(web_server: &WebServer) -> usize {
    let configured = web_server.worker_threads.unwrap_or_else(num_cpus::get);
    if configured == 0 { 1 } else { configured }
}
#[allow(dead_code)] // Useful for callers that already hold a runtime
pub(crate) async fn start(config_path: Option<PathBuf>) -> anyhow::Result<()> {
    let config = load_config(config_path)?;
    start_with_config(config).await
}

pub(crate) async fn start_with_config(config: PkglyConfig) -> anyhow::Result<()> {
    let PkglyConfig {
        web_server,
        database,
        log,
        opentelemetry,
        mode,
        sessions,
        staging: staging_config,
        site,
        security,
        email,
        suggested_local_storage_path,
    } = config;
    let WebServer {
        bind_address,
        max_upload,
        tls,
        open_api_routes,
        ..
    } = web_server;

    let logger = crate::logging::init(log, opentelemetry)?;

    let site = Pkgly::new(
        mode,
        site,
        security,
        sessions,
        staging_config,
        email,
        database,
        suggested_local_storage_path,
    )
    .await
    .context("Unable to Initialize Website Core")?;

    site.start_session_cleaner();

    let cloned_site = site.clone();
    let app = routes::build_app_router(site, max_upload, open_api_routes);

    if let Some(tls) = tls {
        debug!("Starting TLS server");
        let tls = rustls_server_config(tls.private_key, tls.certificate_chain)
            .context("Failed to create TLS configuration")?;
        start_app_with_tls(tls, app, bind_address).await?;
    } else {
        debug!("Starting non-TLS server");
        start_app(app, bind_address, cloned_site).await?;
    }

    info!("Server shutdown... Goodbye!");
    // TODO: Figure out how to properly shutdown the logger
    drop(logger);
    Ok(())
}
async fn start_app(app: Router, bind: String, site: Pkgly) -> anyhow::Result<()> {
    let listener = TcpListener::bind(bind).await?;
    tracing::debug!("listening on {}", listener.local_addr()?);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(site))
    .await?;
    Ok(())
}
async fn start_app_with_tls(
    tls: Arc<ServerConfig>,
    app: Router,
    bind: String,
) -> anyhow::Result<()> {
    let tls_acceptor = TlsAcceptor::from(tls);
    let tcp_listener = TcpListener::bind(bind).await?;

    pin_mut!(tcp_listener);
    loop {
        let tower_service = app.clone();
        let tls_acceptor = tls_acceptor.clone();

        // Wait for new tcp connection
        let (cnx, addr) = tcp_listener.accept().await?;

        tokio::spawn(async move {
            // Wait for tls handshake to happen
            let Ok(stream) = tls_acceptor.accept(cnx).await else {
                error!("error during tls handshake connection from {}", addr);
                return;
            };
            let stream = TokioIo::new(stream);
            let hyper_service = hyper::service::service_fn(move |request: Request<Incoming>| {
                tower_service.clone().call(request)
            });

            let ret = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                .serve_connection_with_upgrades(stream, hyper_service)
                .await;

            if let Err(err) = ret {
                warn!("error serving connection from {}: {}", addr, err);
            }
        });
    }
}

async fn shutdown_signal(website: Pkgly) {
    let ctrl_c = async {
        if let Err(err) = signal::ctrl_c().await {
            error!(?err, "failed to install Ctrl+C handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(err) => error!(?err, "failed to install terminate signal handler"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("Shutting down");
    website.close().await;
}

fn rustls_server_config(
    key: impl AsRef<Path>,
    cert: impl AsRef<Path>,
) -> anyhow::Result<Arc<ServerConfig>> {
    let mut key_reader = BufReader::new(File::open(key)?);
    let mut cert_reader = BufReader::new(File::open(cert)?);

    let cert_chain = certs(&mut cert_reader).collect::<Result<Vec<_>, _>>()?;
    let mut keys = pkcs8_private_keys(&mut key_reader).collect::<Result<Vec<_>, _>>()?;

    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            cert_chain,
            rustls::pki_types::PrivateKeyDer::Pkcs8(keys.remove(0)),
        )?;

    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(Arc::new(config))
}

#[cfg(test)]
mod tests;
