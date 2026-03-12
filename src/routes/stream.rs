use crate::config::Config;
use crate::error::{AppError, Result};
use crate::index::types::IndexEntry;
use crate::scanner::MediaScanner;
use axum::{
    extract::{Path, State},
    Json,
};
use base64::{engine::general_purpose, Engine as _};
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Serialize)]
pub struct StreamResponse {
    pub streams: Vec<Stream>,
}

#[derive(Debug, Serialize)]
pub struct Stream {
    pub url: String,
    pub title: String,
    pub name: String,
    #[serde(rename = "behaviorHints")]
    pub behavior_hints: BehaviorHints,
}

#[derive(Debug, Serialize)]
pub struct BehaviorHints {
    #[serde(rename = "bingeGroup")]
    pub binge_group: String,
    #[serde(rename = "videoSize", skip_serializing_if = "Option::is_none")]
    pub video_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

#[derive(Clone)]
pub struct StreamState {
    pub scanner: Arc<MediaScanner>,
    pub config: Arc<Config>,
}

/// No-auth stream handler (used when PASSWORD is not configured).
pub async fn stream_handler(
    Path((content_type, id)): Path<(String, String)>,
    State(state): State<StreamState>,
) -> Result<Json<StreamResponse>> {
    stream_inner(content_type, id, state).await
}

/// Token-validated stream handler (used when PASSWORD is configured).
pub async fn stream_handler_authed(
    Path((token, content_type, id)): Path<(String, String, String)>,
    State(state): State<StreamState>,
) -> Result<Json<StreamResponse>> {
    if !state.config.is_valid_token(&token) {
        return Err(AppError::NotFound);
    }
    stream_inner(content_type, id, state).await
}

async fn stream_inner(
    content_type: String,
    id: String,
    state: StreamState,
) -> Result<Json<StreamResponse>> {
    // Strip .json extension from id
    let id = id.strip_suffix(".json").unwrap_or(&id).to_string();

    tracing::debug!("Stream request: type={}, id={}", content_type, id);

    let file_info = if content_type == "series" && id.contains(':') {
        // Parse season:episode from ID (format: tt1234567:1:1)
        let parts: Vec<&str> = id.split(':').collect();
        let imdb_id = parts[0];
        let season = parts
            .get(1)
            .and_then(|s| s.parse::<u16>().ok())
            .ok_or_else(|| AppError::InvalidPath("Invalid season".into()))?;
        let episode = parts
            .get(2)
            .and_then(|s| s.parse::<u16>().ok())
            .ok_or_else(|| AppError::InvalidPath("Invalid episode".into()))?;

        match state.scanner.index.get_episode(imdb_id, season, episode) {
            Some(file_info) => {
                tracing::info!(
                    "File found. IMDb={}, Season={}, Episode={}, Path={}",
                    id,
                    season,
                    episode,
                    file_info.file_path.to_string_lossy()
                );
                file_info
            }
            None => {
                tracing::info!(
                    "File not found. IMDb={}, Season={}, Episode={}",
                    imdb_id,
                    season,
                    episode
                );
                return Ok(Json(StreamResponse { streams: vec![] }));
            }
        }
    } else {
        // For movies
        match state.scanner.index.get(&id) {
            Some(IndexEntry::Movie(file_info)) => {
                tracing::info!(
                    "File found. IMDb={} Path={}",
                    id,
                    file_info.file_path.to_string_lossy()
                );
                file_info
            }
            Some(IndexEntry::Series(_)) => {
                return Err(AppError::InvalidPath(
                    "Series requires season:episode format".into(),
                ))
            }
            None => {
                tracing::info!("File not found. IMDb={}", id);
                return Ok(Json(StreamResponse { streams: vec![] }));
            }
        }
    };

    tracing::debug!("  Found: {} ({:?})", file_info.title, file_info.file_path);

    // Encode file path for URL
    let encoded_path =
        general_purpose::STANDARD.encode(file_info.file_path.to_string_lossy().as_bytes());

    // Construct stream URL
    let base_url = state
        .config
        .base_url
        .as_ref()
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_else(|| format!("http://localhost:{}", state.config.port));

    let stream_url = if let Some(ref token) = state.config.auth_token {
        format!("{}/{}/video?path={}", base_url, token, encoded_path)
    } else {
        format!("{}/video?path={}", base_url, encoded_path)
    };

    let mut display = format!("🎞️ {}", file_info.title);
    if let Some(file_name) = file_info.file_path.file_name().and_then(|n| n.to_str()) {
        display = format!("{display}\n📦 {file_name}");
    }

    // Get file size and filename
    let file_size = std::fs::metadata(&file_info.file_path)
        .ok()
        .map(|m| m.len());

    let filename = file_info
        .file_path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string());

    if let Some(size) = file_size {
        tracing::debug!(
            "File size: {} bytes ({:.2} MB)",
            size,
            size as f64 / 1_048_576.0
        );
    }

    let stream = Stream {
        url: stream_url,
        title: display,
        name: "🏠 Lanio".to_string(),
        behavior_hints: BehaviorHints {
            binge_group: "lanio".to_string(),
            video_size: file_size,
            filename,
        },
    };

    Ok(Json(StreamResponse {
        streams: vec![stream],
    }))
}
