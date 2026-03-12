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
use tokio::signal;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lanio=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().without_time())
        .init();

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
    if config.auth_token.is_some() {
        tracing::info!("Authentication: enabled (PASSWORD is set)");
    } else {
        tracing::info!("Authentication: disabled");
    }
    tracing::info!("{}", separator);

    let config = Arc::new(config);

    // Initialize components
    let tmdb_client = Arc::new(TmdbClient::new(config.tmdb_api_key.clone()));
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

    // Start server
    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("{}", separator);
    tracing::info!("Server running on {}", addr);
    tracing::info!("Manifest URL: {}/manifest.json", server_url);
    tracing::info!("Health check: {}/health", server_url);
    tracing::info!("{}", separator);
    tracing::info!("");
    tracing::info!("To add this add-on to Stremio:");
    tracing::info!("1. Open Stremio");
    tracing::info!("2. Go to Add-ons (puzzle icon)");
    tracing::info!("3. Click \"Community Add-ons\"");
    tracing::info!("4. Paste this URL: {}/manifest.json", server_url);
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
