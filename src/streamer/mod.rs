pub mod range;

use crate::config::Config;
use crate::error::{AppError, Result};
use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
};
use base64::{engine::general_purpose, Engine as _};
use range::parse_range_header;
use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

fn validate_path(file_path: &FsPath, media_path: &FsPath) -> Result<PathBuf> {
    let resolved = file_path.canonicalize().map_err(|e| {
        AppError::InvalidPath(format!("Cannot resolve path: {}", e))
    })?;

    let resolved_media = media_path
        .canonicalize()
        .map_err(|e| AppError::InvalidPath(format!("Cannot resolve media path: {}", e)))?;

    if !resolved.starts_with(&resolved_media) {
        return Err(AppError::InvalidPath(
            "Path is outside media directory".into(),
        ));
    }

    Ok(resolved)
}

#[derive(Clone)]
pub struct StreamerState {
    pub config: Arc<Config>,
}

pub async fn video_handler(
    Query(params): Query<HashMap<String, String>>,
    State(state): State<StreamerState>,
    headers: HeaderMap,
) -> Result<Response> {
    video_inner(params, state, headers).await
}

async fn video_inner(
    params: HashMap<String, String>,
    state: StreamerState,
    headers: HeaderMap,
) -> Result<Response> {
    // Decode base64 path
    let encoded_path = params
        .get("path")
        .ok_or_else(|| AppError::InvalidPath("Missing path parameter".into()))?;

    let path_bytes = general_purpose::STANDARD
        .decode(encoded_path)
        .map_err(|_| AppError::InvalidPath("Invalid base64 encoding".into()))?;

    let file_path_str = String::from_utf8(path_bytes)
        .map_err(|_| AppError::InvalidPath("Invalid UTF-8 in path".into()))?;

    let file_path = PathBuf::from(&file_path_str);

    // Validate path
    let validated_path = validate_path(&file_path, &state.config.media_path)?;

    // Open file
    let mut file = File::open(&validated_path).await?;
    let metadata = file.metadata().await?;
    let file_size = metadata.len();

    // Get MIME type
    let mime_type = mime_guess::from_path(&validated_path)
        .first_or_octet_stream()
        .to_string();

    // Check for Range header
    if let Some(range_header) = headers.get(header::RANGE) {
        let range_str = range_header
            .to_str()
            .map_err(|_| AppError::InvalidPath("Invalid range header".into()))?;

        let (start, end) = parse_range_header(range_str, file_size)?;
        let content_length = end - start + 1;

        // Seek to start position
        file.seek(std::io::SeekFrom::Start(start)).await?;

        // Create a limited reader
        let limited = file.take(content_length);
        let stream = ReaderStream::new(limited);

        // Return 206 Partial Content
        Ok(Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_TYPE, mime_type)
            .header(header::CONTENT_LENGTH, content_length)
            .header(
                header::CONTENT_RANGE,
                format!("bytes {}-{}/{}", start, end, file_size),
            )
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, OPTIONS")
            .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "Range")
            .header(
                header::ACCESS_CONTROL_EXPOSE_HEADERS,
                "Content-Length, Content-Range, Accept-Ranges",
            )
            .body(Body::from_stream(stream))
            .unwrap())
    } else {
        // Return full file
        let stream = ReaderStream::new(file);

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime_type)
            .header(header::CONTENT_LENGTH, file_size)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, OPTIONS")
            .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "Range")
            .header(
                header::ACCESS_CONTROL_EXPOSE_HEADERS,
                "Content-Length, Content-Range, Accept-Ranges",
            )
            .body(Body::from_stream(stream))
            .unwrap())
    }
}
