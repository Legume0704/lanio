mod common;

use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use base64::{engine::general_purpose, Engine as _};
use common::{encode_path, get_json, get_raw, setup, MOVIE_IMDB_ID};
use lanio::auth::compute_token;

#[tokio::test]
async fn video_serves_full_file() {
    let app = setup(None).await;
    let file_path = app
        ._tmpdir
        .path()
        .join(format!("The.Dark.Knight.2008.{MOVIE_IMDB_ID}.mkv"));
    // The path on disk is canonicalized by the scanner; we must match that.
    let canonical = file_path.canonicalize().unwrap();
    let encoded = encode_path(&canonical);

    let (status, body, headers) =
        get_raw(&app, &format!("/video?path={encoded}"), HeaderMap::new()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"fake");
    assert!(
        headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .contains("video"),
        "expected a video/* MIME type"
    );
    assert_eq!(
        headers
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok()),
        Some("4")
    );
}

#[tokio::test]
async fn video_serves_partial_content_via_range() {
    let app = setup(None).await;
    let file_path = app
        ._tmpdir
        .path()
        .join(format!("The.Dark.Knight.2008.{MOVIE_IMDB_ID}.mkv"));
    let canonical = file_path.canonicalize().unwrap();
    let encoded = encode_path(&canonical);

    let mut headers = HeaderMap::new();
    headers.insert(header::RANGE, HeaderValue::from_static("bytes=0-1"));

    let (status, body, resp_headers) =
        get_raw(&app, &format!("/video?path={encoded}"), headers).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(body, b"fa"); // first 2 bytes of b"fake"
    assert_eq!(
        resp_headers
            .get(header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok()),
        Some("bytes 0-1/4")
    );
}

#[tokio::test]
async fn video_stream_url_round_trip() {
    // End-to-end: ask the stream endpoint for a URL, then hit that URL and
    // confirm the actual file bytes come back.
    let app = setup(None).await;
    let (_, json) = get_json(&app, &format!("/stream/movie/{MOVIE_IMDB_ID}")).await;
    let full_url = json["streams"][0]["url"].as_str().unwrap();

    // Strip the scheme+host so the in-process router can handle it.
    let path = full_url.trim_start_matches("http://localhost:8078");

    let (status, body, _) = get_raw(&app, path, HeaderMap::new()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"fake");
}

#[tokio::test]
async fn video_rejects_path_traversal() {
    let app = setup(None).await;
    // Encode a path that lives outside the media directory.
    let encoded = general_purpose::STANDARD.encode(b"/etc/passwd");
    let (status, _, _) =
        get_raw(&app, &format!("/video?path={encoded}"), HeaderMap::new()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn video_stream_url_includes_token_when_auth_enabled() {
    let app = setup(Some("testpassword")).await;
    let token = compute_token("testpassword");
    let (_, json) =
        get_json(&app, &format!("/{token}/stream/movie/{MOVIE_IMDB_ID}")).await;
    let url = json["streams"][0]["url"].as_str().unwrap();
    assert!(
        url.contains(&format!("/{token}/video")),
        "stream URL should include token; got: {url}"
    );
}

#[tokio::test]
async fn video_accessible_via_token_when_auth_enabled() {
    // Round-trip with auth: stream URL (which includes the token) should serve the file.
    let app = setup(Some("testpassword")).await;
    let token = compute_token("testpassword");
    let (_, json) =
        get_json(&app, &format!("/{token}/stream/movie/{MOVIE_IMDB_ID}")).await;
    let full_url = json["streams"][0]["url"].as_str().unwrap();
    let path = full_url.trim_start_matches("http://localhost:8078");

    let (status, body, _) = get_raw(&app, path, HeaderMap::new()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"fake");
}
