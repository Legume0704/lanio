#![allow(dead_code)]

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::Router;
use base64::{engine::general_purpose, Engine as _};
use http_body_util::BodyExt;
use lanio::auth::compute_token;
use lanio::config::Config;
use lanio::index::MediaIndex;
use lanio::metadata::TmdbClient;
use lanio::routes;
use lanio::scanner::MediaScanner;
use serde_json::Value;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

// Stable IMDb IDs embedded in test filenames so the scanner uses
// `get_metadata_by_imdb_id` directly (one TMDB round-trip per file).
pub const MOVIE_IMDB_ID: &str = "tt0468569";
pub const SERIES_IMDB_ID: &str = "tt0903747";

pub struct TestApp {
    pub router: Router,
    /// Exposed so video tests can resolve file paths via `_tmpdir.path()`.
    /// Underscore prefix suppresses dead_code in binaries that don't access it.
    pub _tmpdir: TempDir,
    _mock_server: MockServer,
}

pub async fn setup(password: Option<&str>) -> TestApp {
    // 1. Start a local TMDB mock server. The scanner calls
    //    GET /find/{imdb_id}?… — we return the minimum valid response so the
    //    IMDb ID is indexed (poster is optional; an empty result is fine).
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/find/tt\d+"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "movie_results": [],
            "tv_results": []
        })))
        .mount(&mock_server)
        .await;

    // 2. Build a temporary media directory with one movie and one series episode.
    let tmpdir = TempDir::new().unwrap();
    let media = tmpdir.path();

    // Movie: IMDb ID embedded directly in the filename.
    fs::write(
        media.join(format!("The.Dark.Knight.2008.{}.mkv", MOVIE_IMDB_ID)),
        b"fake",
    )
    .unwrap();

    // Series: episode file inside a named subdirectory. The scanner picks up
    // the IMDb ID from the filename.
    let series_dir = media.join("Breaking.Bad");
    fs::create_dir(&series_dir).unwrap();
    fs::write(
        series_dir.join(format!("Breaking.Bad.S01E01.{}.mkv", SERIES_IMDB_ID)),
        b"fake",
    )
    .unwrap();

    // 3. Build config (no env lookup — construct directly like unit tests do).
    let auth_token = password.map(compute_token);
    let config = Arc::new(Config {
        media_path: media.to_path_buf(),
        port: 8078,
        base_url: Some("http://localhost:8078".to_string()),
        public_url: None,
        tmdb_api_key: "test_key".to_string(),
        password: password.map(str::to_string),
        auth_token,
    });

    // 4. Build scanner pointed at the mock TMDB server.
    let tmdb = Arc::new(TmdbClient::new_with_base_url(
        "test_key".to_string(),
        mock_server.uri(),
    ));
    let index = Arc::new(MediaIndex::new());
    let scanner = Arc::new(MediaScanner::new(index, tmdb, Arc::clone(&config)));

    // 5. Scan synchronously — no file watcher, just the initial directory pass.
    scanner.scan().await.unwrap();

    let router = routes::create_router(scanner, config);
    TestApp {
        router,
        _tmpdir: tmpdir,
        _mock_server: mock_server,
    }
}

pub async fn get_json(app: &TestApp, uri: &str) -> (StatusCode, Value) {
    let response = app
        .router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

pub async fn get_status(app: &TestApp, uri: &str) -> StatusCode {
    app.router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

pub async fn get_raw(
    app: &TestApp,
    uri: &str,
    extra_headers: HeaderMap,
) -> (StatusCode, Vec<u8>, HeaderMap) {
    let mut builder = Request::builder().uri(uri);
    for (name, value) in &extra_headers {
        builder = builder.header(name, value);
    }
    let response = app
        .router
        .clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let resp_headers = response.headers().clone();
    let bytes = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, bytes, resp_headers)
}

/// Base64-encode an absolute file path the same way the stream handler does.
pub fn encode_path(path: &std::path::Path) -> String {
    general_purpose::STANDARD.encode(path.to_string_lossy().as_bytes())
}
