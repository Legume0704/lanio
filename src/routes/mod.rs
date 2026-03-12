pub mod catalog;
pub mod health;
pub mod home;
pub mod manifest;
pub mod stream;

use crate::config::Config;
use crate::scanner::MediaScanner;
use crate::streamer::StreamerState;
use axum::{routing::get, Router};
use catalog::{catalog_handler, catalog_handler_authed, CatalogState};
use health::health_handler;
use home::home_handler;
use manifest::{get_manifest, get_manifest_authed};
use std::sync::Arc;
use stream::{stream_handler, stream_handler_authed, StreamState};
use tower_http::cors::CorsLayer;
use crate::streamer::video_handler_authed;

pub fn create_router(scanner: Arc<MediaScanner>, config: Arc<Config>) -> Router {
    let stream_state = StreamState {
        scanner: Arc::clone(&scanner),
        config: Arc::clone(&config),
    };

    let catalog_state = CatalogState {
        scanner: Arc::clone(&scanner),
        config: Arc::clone(&config),
    };

    let streamer_state = StreamerState {
        config: Arc::clone(&config),
    };

    if config.auth_token.is_some() {
        // Auth enabled: Stremio routes are prefixed with /:token
        Router::new()
            .route("/", get(home_handler))
            .with_state(Arc::clone(&config))
            .route("/:token/manifest.json", get(get_manifest_authed))
            .with_state(Arc::clone(&config))
            .route("/:token/stream/:type/:id", get(stream_handler_authed))
            .with_state(stream_state)
            .route("/:token/catalog/:type/:id", get(catalog_handler_authed))
            .with_state(catalog_state)
            .route("/:token/video", get(video_handler_authed))
            .with_state(streamer_state)
            .route("/health", get(health_handler))
            .with_state(scanner)
            .layer(CorsLayer::permissive())
    } else {
        // No auth: original routes
        Router::new()
            .route("/", get(home_handler))
            .with_state(Arc::clone(&config))
            .route("/manifest.json", get(get_manifest))
            .route("/stream/:type/:id", get(stream_handler))
            .with_state(stream_state)
            .route("/catalog/:type/:id", get(catalog_handler))
            .with_state(catalog_state)
            .route("/video", get(crate::streamer::video_handler))
            .with_state(streamer_state)
            .route("/health", get(health_handler))
            .with_state(scanner)
            .layer(CorsLayer::permissive())
    }
}
