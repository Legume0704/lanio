mod common;

use axum::http::StatusCode;
use common::{get_json, setup, MOVIE_IMDB_ID, SERIES_IMDB_ID};

#[tokio::test]
async fn movie_stream_returns_url() {
    let app = setup(None).await;
    let (status, json) = get_json(&app, &format!("/stream/movie/{MOVIE_IMDB_ID}")).await;
    assert_eq!(status, StatusCode::OK);
    let streams = json["streams"].as_array().unwrap();
    assert!(!streams.is_empty(), "expected at least one stream");
    let url = streams[0]["url"].as_str().unwrap();
    assert!(url.contains("/video?path="), "unexpected stream URL: {url}");
}

#[tokio::test]
async fn series_stream_returns_url() {
    let app = setup(None).await;
    let (status, json) =
        get_json(&app, &format!("/stream/series/{SERIES_IMDB_ID}:1:1")).await;
    assert_eq!(status, StatusCode::OK);
    let streams = json["streams"].as_array().unwrap();
    assert!(!streams.is_empty(), "expected at least one stream");
    let url = streams[0]["url"].as_str().unwrap();
    assert!(url.contains("/video?path="), "unexpected stream URL: {url}");
}

#[tokio::test]
async fn missing_stream_returns_empty_streams() {
    let app = setup(None).await;
    let (status, json) = get_json(&app, "/stream/movie/tt9999999").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["streams"].as_array().unwrap().len(), 0);
}
