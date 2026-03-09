pub mod catalog;
pub mod health;
pub mod home;
pub mod manifest;
pub mod stream;

use crate::config::Config;
use crate::scanner::MediaScanner;
use crate::streamer::StreamerState;
use axum::{routing::get, Router};
use catalog::{catalog_handler, CatalogState};
use health::health_handler;
use home::home_handler;
use manifest::get_manifest;
use std::sync::Arc;
use stream::{stream_handler, StreamState};
use tower_http::cors::CorsLayer;

pub fn create_router(scanner: Arc<MediaScanner>, config: Arc<Config>) -> Router {
    let stream_state = StreamState {
        scanner: Arc::clone(&scanner),
        config: Arc::clone(&config),
    };

    let catalog_state = CatalogState {
        scanner: Arc::clone(&scanner),
    };

    let streamer_state = StreamerState {
        config: Arc::clone(&config),
    };

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
