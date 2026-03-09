use crate::scanner::MediaScanner;
use axum::{extract::State, Json};
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub scanning: bool,
    pub stats: Stats,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub movies: usize,
    pub series: usize,
    pub episodes: usize,
}

pub async fn health_handler(State(scanner): State<Arc<MediaScanner>>) -> Json<HealthResponse> {
    let stats = scanner.index.stats();

    Json(HealthResponse {
        status: "ok".to_string(),
        scanning: scanner.scanning.load(Ordering::SeqCst),
        stats: Stats {
            movies: stats.movies,
            series: stats.series,
            episodes: stats.episodes,
        },
    })
}
