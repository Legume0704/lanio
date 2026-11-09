mod common;

use common::{catalog_metas, mock_movie, start_server_with_unmatched, wait_until};
use httpmock::prelude::*;
use std::time::Duration;
use tempfile::tempdir;
use tokio::sync::{Mutex, MutexGuard};

const UNMATCHED_CATALOG: &str = "lanio-unmatched";

/// The server reads its configuration from environment variables, which are
/// shared by every test in this binary, so tests must not overlap.
static ENV_LOCK: Mutex<()> = Mutex::const_new(());

async fn env_guard() -> MutexGuard<'static, ()> {
    ENV_LOCK.lock().await
}

#[tokio::test]
async fn unmatched_files_are_listed_and_streamable_when_enabled() {
    let _guard = env_guard().await;
    let tmdb = MockServer::start();
    mock_movie(&tmdb, "Sintel", 5678, "tt1727596", None);

    let temp_media = tempdir().unwrap();
    std::fs::write(
        temp_media.path().join("Sintel.2010.mp4"),
        "fake video data for Sintel",
    )
    .unwrap();
    std::fs::write(
        temp_media.path().join("2024 Brazilian Grand Prix.ts"),
        "fake sports recording",
    )
    .unwrap();
    // Non-video sidecars must not be listed.
    std::fs::write(
        temp_media.path().join("2024 Brazilian Grand Prix.nfo"),
        "metadata",
    )
    .unwrap();
    std::fs::write(
        temp_media.path().join("2024 Brazilian Grand Prix.srt"),
        "subtitles",
    )
    .unwrap();
    std::fs::write(temp_media.path().join("poster.jpg"), "artwork").unwrap();

    let base_url =
        start_server_with_unmatched(temp_media.path(), &tmdb.base_url(), None, None, true).await;
    let client = reqwest::Client::new();

    let listed = wait_until(20, Duration::from_millis(500), || async {
        catalog_metas(&client, &base_url, "other", UNMATCHED_CATALOG)
            .await
            .iter()
            .any(|meta| meta["name"] == "2024 Brazilian Grand Prix.ts")
    })
    .await;
    assert!(listed, "unmatched recording never appeared in its catalog");

    let metas = catalog_metas(&client, &base_url, "other", UNMATCHED_CATALOG).await;
    assert_eq!(
        metas.len(),
        1,
        "only known video file types should be listed"
    );

    let meta = &metas[0];
    let id = meta["id"].as_str().expect("unmatched meta needs an id");
    assert!(
        id.starts_with("lanio-"),
        "unmatched files need their own ID namespace"
    );
    assert_eq!(meta["type"], "other");
    assert_eq!(meta["name"], "2024 Brazilian Grand Prix.ts");
    assert!(meta.get("poster").is_none());

    // The existing matched-movie catalog is untouched.
    let movies = catalog_metas(&client, &base_url, "movie", "lanio-movies").await;
    assert!(
        movies.iter().any(|movie| movie["name"] == "Sintel"),
        "matching movies must keep working alongside the unmatched catalog"
    );

    // The detail page carries a directly playable stream.
    let meta_response: serde_json::Value = client
        .get(format!("{}/meta/other/{}", base_url, id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let stream_url = meta_response["meta"]["videos"][0]["streams"][0]["url"]
        .as_str()
        .expect("unmatched detail page must expose a stream")
        .to_string();

    let video_response = client.get(stream_url).send().await.unwrap();
    assert!(video_response.status().is_success());
    assert_eq!(
        video_response.text().await.unwrap(),
        "fake sports recording",
        "the file must be served as-is"
    );

    // The stream resource works on its own too.
    let stream_response: serde_json::Value = client
        .get(format!("{}/stream/other/{}", base_url, id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(stream_response["streams"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn unmatched_catalog_is_absent_by_default() {
    let _guard = env_guard().await;
    let tmdb = MockServer::start();
    let temp_media = tempdir().unwrap();
    std::fs::write(
        temp_media.path().join("2024 Brazilian Grand Prix.ts"),
        "fake sports recording",
    )
    .unwrap();

    let base_url =
        start_server_with_unmatched(temp_media.path(), &tmdb.base_url(), None, None, false).await;
    let client = reqwest::Client::new();

    let manifest: serde_json::Value = client
        .get(format!("{}/manifest.json", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        !manifest["catalogs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|catalog| catalog["id"] == UNMATCHED_CATALOG),
        "the unmatched catalog must be opt-in"
    );

    let metas = catalog_metas(&client, &base_url, "other", UNMATCHED_CATALOG).await;
    assert!(metas.is_empty());

    let stream_response: serde_json::Value = client
        .get(format!("{}/stream/other/lanio-missing", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(stream_response["streams"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn new_unmatched_file_appears_and_removed_file_disappears() {
    let _guard = env_guard().await;
    let tmdb = MockServer::start();
    let temp_media = tempdir().unwrap();

    let base_url =
        start_server_with_unmatched(temp_media.path(), &tmdb.base_url(), None, None, true).await;
    let client = reqwest::Client::new();

    let file = temp_media.path().join("Recording.ts");
    std::fs::write(&file, "recording data").unwrap();

    let appeared = wait_until(20, Duration::from_millis(500), || async {
        !catalog_metas(&client, &base_url, "other", UNMATCHED_CATALOG)
            .await
            .is_empty()
    })
    .await;
    assert!(appeared, "newly created recording never appeared");

    std::fs::remove_file(&file).unwrap();

    let disappeared = wait_until(20, Duration::from_millis(500), || async {
        catalog_metas(&client, &base_url, "other", UNMATCHED_CATALOG)
            .await
            .is_empty()
    })
    .await;
    assert!(disappeared, "removed recording is still listed");
}
