pub mod catalog;
pub mod health;
pub mod home;
pub mod manifest;
pub mod stream;

use crate::config::Config;
use crate::scanner::MediaScanner;
use crate::streamer::{video_handler, StreamerState};
use axum::{
    extract::{Path, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::collections::HashMap;
use catalog::{catalog_handler, CatalogState};
use health::health_handler;
use home::home_handler;
use manifest::get_manifest;
use std::sync::Arc;
use stream::{stream_handler, StreamState};
use tower_http::cors::CorsLayer;

async fn token_auth(
    Path(params): Path<HashMap<String, String>>,
    State(config): State<Arc<Config>>,
    request: Request,
    next: Next,
) -> Response {
    let token = params.get("token").map(|s| s.as_str()).unwrap_or("");
    if config.is_valid_token(token) {
        next.run(request).await
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

fn build_stremio_subrouter(scanner: &Arc<MediaScanner>, config: &Arc<Config>) -> Router {
    let stream_state = StreamState {
        scanner: Arc::clone(scanner),
        config: Arc::clone(config),
    };
    let catalog_state = CatalogState {
        scanner: Arc::clone(scanner),
    };
    let streamer_state = StreamerState {
        config: Arc::clone(config),
    };

    let subrouter = Router::new()
        .route("/manifest.json", get(get_manifest))
        .route("/stream/:type/:id", get(stream_handler))
        .with_state(stream_state)
        .route("/catalog/:type/:id", get(catalog_handler))
        .with_state(catalog_state)
        .route("/video", get(video_handler))
        .with_state(streamer_state);

    if config.auth_token.is_some() {
        Router::new().nest(
            "/:token",
            subrouter.route_layer(middleware::from_fn_with_state(
                Arc::clone(config),
                token_auth,
            )),
        )
    } else {
        subrouter
    }
}

pub fn create_router(scanner: Arc<MediaScanner>, config: Arc<Config>) -> Router {
    Router::new()
        .route("/", get(home_handler))
        .with_state(Arc::clone(&config))
        .merge(build_stremio_subrouter(&scanner, &config))
        .merge(
            Router::new()
                .route("/health", get(health_handler))
                .with_state(scanner),
        )
        .layer(CorsLayer::permissive())
}
