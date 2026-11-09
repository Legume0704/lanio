use crate::config::Config;
use crate::index::types::UnmatchedFile;
use crate::routes::stream::{authenticated_url, StreamState};
use crate::scanner::poster_sidecar;
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

/// URL for an unmatched file's poster sidecar, if one sits beside the video.
pub fn poster_url(config: &Config, file: &UnmatchedFile) -> Option<String> {
    poster_sidecar(&file.file_path)
        .map(|_| authenticated_url(config, &format!("poster/{}", file.id)))
}

pub async fn poster_handler(Path(id): Path<String>, State(state): State<StreamState>) -> Response {
    poster_inner(&id, &state).await
}

async fn poster_inner(id: &str, state: &StreamState) -> Response {
    let id = id.strip_suffix(".json").unwrap_or(id);

    if !state.config.unmatched_catalog {
        return StatusCode::NOT_FOUND.into_response();
    }

    // Only images matched by name to a known unmatched video are served, so
    // this route cannot read arbitrary files.
    let Some(file) = state.scanner.index.get_unmatched(id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(poster) = poster_sidecar(&file.file_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Ok(bytes) = tokio::fs::read(&poster).await else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let extension = poster
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let content_type = mime_guess::from_ext(&extension)
        .first_or_octet_stream()
        .to_string();

    (
        [
            (header::CONTENT_TYPE, content_type),
            // Sidecars can be replaced while the server is running.
            (header::CACHE_CONTROL, "no-cache".to_string()),
        ],
        bytes,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::MediaIndex;
    use crate::metadata::TmdbClient;
    use crate::scanner::MediaScanner;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn make_state(unmatched_catalog: bool) -> StreamState {
        let config = Arc::new(Config {
            media_path: PathBuf::from("/media"),
            port: 8078,
            base_url: Some("http://lan.local:8078".to_string()),
            public_url: None,
            tmdb_api_key: "fake".to_string(),
            tmdb_base_url: "http://localhost".to_string(),
            tmdb_image_base_url: "http://localhost".to_string(),
            password: None,
            auth_token: None,
            poster_url: None,
            scan_cron: None,
            unmatched_catalog,
        });

        let index = Arc::new(MediaIndex::new());
        let tmdb_client = Arc::new(TmdbClient::new(
            "fake".to_string(),
            "http://localhost".to_string(),
            "http://localhost".to_string(),
        ));
        let scanner = Arc::new(MediaScanner::new(index, tmdb_client, Arc::clone(&config)));

        StreamState { scanner, config }
    }

    fn insert_file(state: &StreamState, video: &std::path::Path) {
        state.scanner.index.insert_unmatched(UnmatchedFile {
            id: "lanio-abc123".to_string(),
            title: video.file_name().unwrap().to_string_lossy().to_string(),
            file_path: video.to_path_buf(),
            created: Some(1000),
        });
    }

    #[tokio::test]
    async fn poster_url_is_absent_without_a_sidecar() {
        let dir = tempdir().unwrap();
        let video = dir.path().join("game.ts");
        std::fs::write(&video, b"").unwrap();

        let state = make_state(true);
        insert_file(&state, &video);
        let file = state.scanner.index.get_unmatched("lanio-abc123").unwrap();

        assert_eq!(poster_url(&state.config, &file), None);
    }

    #[tokio::test]
    async fn poster_url_points_at_the_poster_route() {
        let dir = tempdir().unwrap();
        let video = dir.path().join("game.ts");
        std::fs::write(&video, b"").unwrap();
        std::fs::write(dir.path().join("game.jpg"), b"image").unwrap();

        let state = make_state(true);
        insert_file(&state, &video);
        let file = state.scanner.index.get_unmatched("lanio-abc123").unwrap();

        assert_eq!(
            poster_url(&state.config, &file).as_deref(),
            Some("http://lan.local:8078/poster/lanio-abc123")
        );
    }

    #[tokio::test]
    async fn poster_route_serves_the_matching_image() {
        let dir = tempdir().unwrap();
        let video = dir.path().join("game.ts");
        std::fs::write(&video, b"").unwrap();
        std::fs::write(dir.path().join("game.jpg"), b"jpeg-bytes").unwrap();

        let state = make_state(true);
        insert_file(&state, &video);

        let response = poster_inner("lanio-abc123", &state).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/jpeg"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"jpeg-bytes");
    }

    #[tokio::test]
    async fn poster_route_ignores_images_for_other_files() {
        let dir = tempdir().unwrap();
        let video = dir.path().join("game.ts");
        std::fs::write(&video, b"").unwrap();
        std::fs::write(dir.path().join("poster.jpg"), b"unrelated").unwrap();

        let state = make_state(true);
        insert_file(&state, &video);

        let response = poster_inner("lanio-abc123", &state).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn poster_route_is_not_available_when_disabled() {
        let dir = tempdir().unwrap();
        let video = dir.path().join("game.ts");
        std::fs::write(&video, b"").unwrap();
        std::fs::write(dir.path().join("game.jpg"), b"image").unwrap();

        let state = make_state(false);
        insert_file(&state, &video);

        let response = poster_inner("lanio-abc123", &state).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn poster_route_404s_for_an_unknown_id() {
        let state = make_state(true);
        let response = poster_inner("lanio-missing", &state).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
