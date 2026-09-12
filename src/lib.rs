mod auth;
mod config;
mod error;
mod index;
mod metadata;
mod routes;
mod scanner;
mod streamer;

use config::Config;
use index::MediaIndex;
use metadata::TmdbClient;
use scanner::MediaScanner;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::signal;

/// Runs the Lanio add-on server.
/// If `listener` is provided, the server will bind to it.
/// Otherwise, it will bind to the port specified in the configuration.
pub async fn run(listener: Option<TcpListener>) -> anyhow::Result<()> {
    let separator = "=".repeat(50);
    tracing::info!("{}", separator);
    tracing::info!("Lanio Add-on");
    tracing::info!("{}", separator);

    // Load configuration
    let config = Config::from_env()?;
    tracing::info!("Media path: {:?}", config.media_path);
    tracing::info!("Port: {}", config.port);
    if let Some(ref base_url) = config.base_url {
        tracing::info!("Base URL: {}", base_url);
    }
    if let Some(ref public_url) = config.public_url {
        tracing::info!("Public URL: {}", public_url);
    }
    if let Some(ref poster_url) = config.poster_url {
        tracing::info!("Poster URL: {}", poster_url);
    }
    if config.auth_token.is_some() {
        tracing::info!("Authentication: enabled (PASSWORD is set)");
    } else {
        tracing::info!("Authentication: disabled");
    }
    tracing::info!("{}", separator);

    let config = Arc::new(config);

    // Initialize components
    let tmdb_client = Arc::new(TmdbClient::new(
        config.tmdb_api_key.clone(),
        config.tmdb_base_url.clone(),
        config.tmdb_image_base_url.clone(),
    ));
    let index = Arc::new(MediaIndex::new());
    let scanner = Arc::new(MediaScanner::new(
        Arc::clone(&index),
        Arc::clone(&tmdb_client),
        Arc::clone(&config),
    ));

    // Start scanner
    scanner.start().await;

    // Create router
    let app = routes::create_router(Arc::clone(&scanner), Arc::clone(&config));

    // Get server URL for display (public_url > base_url > local)
    let server_url = config
        .public_url
        .as_ref()
        .or(config.base_url.as_ref())
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_else(|| {
            format!(
                "http://{}:{}",
                std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".to_string()),
                config.port
            )
        });

    // Determine listener
    let listener = match listener {
        Some(l) => l,
        None => {
            let addr = format!("0.0.0.0:{}", config.port);
            TcpListener::bind(&addr).await?
        }
    };

    let local_addr = listener.local_addr()?;
    tracing::info!("{}", separator);
    tracing::info!("Server running on {}", local_addr);
    tracing::info!("Public URL: {}", server_url);
    tracing::info!("Health check: http://{}/health", local_addr);
    tracing::info!("{}", separator);

    // Run server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Server shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C, shutting down...");
        },
        _ = terminate => {
            tracing::info!("Received SIGTERM, shutting down...");
        },
    }
}
