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
use range::{parse_range_header, RangeRequest};
use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;
use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, ReadBuf};
use tokio_util::io::ReaderStream;

fn validate_path(file_path: &FsPath, media_path: &FsPath) -> Result<PathBuf> {
    let resolved = file_path
        .canonicalize()
        .map_err(|e| AppError::InvalidPath(format!("Cannot resolve path: {}", e)))?;

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

fn decode_path_param(encoded: &str) -> Result<String> {
    let path_bytes = general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| AppError::InvalidPath("Invalid base64 encoding".into()))?;

    String::from_utf8(path_bytes).map_err(|_| AppError::InvalidPath("Invalid UTF-8 in path".into()))
}

/// Wraps the response body reader so we can log how much was actually sent
/// once the body is dropped (finished, or the client disconnected).
struct LoggedReader<R> {
    inner: R,
    file_name: String,
    range: String,
    expected: u64,
    sent: u64,
    started: Instant,
}

impl<R: AsyncRead + Unpin> AsyncRead for LoggedReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Err(ref e)) = result {
            tracing::warn!("Read error streaming {}: {}", self.file_name, e);
        }
        self.sent += (buf.filled().len() - before) as u64;
        result
    }
}

impl<R> Drop for LoggedReader<R> {
    fn drop(&mut self) {
        let elapsed = self.started.elapsed().as_secs_f64();
        if self.sent < self.expected {
            // Normal when the player seeks or stops, but a short body during
            // startup is what causes "Loading finished before preparation".
            tracing::debug!(
                "Video stream ended early: file={} range={} sent={}/{} bytes after {:.1}s",
                self.file_name,
                self.range,
                self.sent,
                self.expected,
                elapsed
            );
        } else {
            tracing::debug!(
                "Video stream complete: file={} range={} sent={} bytes in {:.1}s",
                self.file_name,
                self.range,
                self.sent,
                elapsed
            );
        }
    }
}

fn with_common_headers(builder: axum::http::response::Builder) -> axum::http::response::Builder {
    builder
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, OPTIONS")
        .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "Range")
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            "Content-Length, Content-Range, Accept-Ranges",
        )
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
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("none")
        .to_string();
    let result = video_inner(params, state, headers).await;
    match &result {
        Ok(response) => tracing::debug!(
            "Video request: range={} -> {} {}",
            range,
            response.status().as_u16(),
            response
                .headers()
                .get(header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
        ),
        Err(e) => tracing::warn!("Video request failed: range={} -> {}", range, e),
    }
    result
}

async fn video_inner(
    params: HashMap<String, String>,
    state: StreamerState,
    headers: HeaderMap,
) -> Result<Response> {
    let encoded_path = params
        .get("path")
        .ok_or_else(|| AppError::InvalidPath("Missing path parameter".into()))?;

    let file_path = PathBuf::from(decode_path_param(encoded_path)?);

    // Validate path
    let validated_path = validate_path(&file_path, &state.config.media_path)?;

    // Open file
    let mut file = File::open(&validated_path).await?;
    let metadata = file.metadata().await?;
    let file_size = metadata.len();

    let file_name = validated_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    tracing::debug!("Serving {} ({} bytes)", file_name, file_size);

    // Get MIME type
    let mime_type = mime_guess::from_path(&validated_path)
        .first_or_octet_stream()
        .to_string();

    let range_header = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let range = match range_header {
        Some(r) => parse_range_header(r, file_size),
        None => RangeRequest::Ignored,
    };

    match range {
        RangeRequest::Satisfiable(start, end) => {
            let content_length = end - start + 1;

            // Seek to start position
            file.seek(std::io::SeekFrom::Start(start)).await?;

            let reader = LoggedReader {
                inner: file.take(content_length),
                file_name,
                range: format!("{}-{}", start, end),
                expected: content_length,
                sent: 0,
                started: Instant::now(),
            };

            // Return 206 Partial Content
            Ok(with_common_headers(Response::builder())
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, mime_type)
                .header(header::CONTENT_LENGTH, content_length)
                .header(
                    header::CONTENT_RANGE,
                    format!("bytes {}-{}/{}", start, end, file_size),
                )
                .body(Body::from_stream(ReaderStream::new(reader)))
                .unwrap())
        }
        RangeRequest::Unsatisfiable => Ok(with_common_headers(Response::builder())
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{}", file_size))
            .body(Body::empty())
            .unwrap()),
        RangeRequest::Ignored => {
            let reader = LoggedReader {
                inner: file,
                file_name,
                range: "full".to_string(),
                expected: file_size,
                sent: 0,
                started: Instant::now(),
            };

            // Return full file
            Ok(with_common_headers(Response::builder())
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime_type)
                .header(header::CONTENT_LENGTH, file_size)
                .body(Body::from_stream(ReaderStream::new(reader)))
                .unwrap())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_url_safe_paths() {
        // Chosen so the standard encoding would contain '+' and '/'
        let path = "/media/TV/Show?>/S01E01~.mkv";
        let encoded = general_purpose::URL_SAFE_NO_PAD.encode(path);
        assert!(!encoded.contains('+') && !encoded.contains('/'));
        assert_eq!(decode_path_param(&encoded).unwrap(), path);
    }
}
