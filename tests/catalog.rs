mod common;

use axum::http::StatusCode;
use common::{get_json, setup, MOVIE_IMDB_ID, SERIES_IMDB_ID};

#[tokio::test]
async fn movie_appears_in_catalog() {
    let app = setup(None).await;
    let (status, json) = get_json(&app, "/catalog/movie/lanio-movies").await;
    assert_eq!(status, StatusCode::OK);
    let metas = json["metas"].as_array().unwrap();
    assert!(
        metas.iter().any(|m| m["id"] == MOVIE_IMDB_ID),
        "expected {MOVIE_IMDB_ID} in movie catalog; got: {metas:?}"
    );
}

#[tokio::test]
async fn series_appears_in_catalog() {
    let app = setup(None).await;
    let (status, json) = get_json(&app, "/catalog/series/lanio-series").await;
    assert_eq!(status, StatusCode::OK);
    let metas = json["metas"].as_array().unwrap();
    assert!(
        metas.iter().any(|m| m["id"] == SERIES_IMDB_ID),
        "expected {SERIES_IMDB_ID} in series catalog; got: {metas:?}"
    );
}

#[tokio::test]
async fn unknown_catalog_returns_empty_metas() {
    let app = setup(None).await;
    let (status, json) = get_json(&app, "/catalog/movie/nonexistent").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["metas"].as_array().unwrap().len(), 0);
}
