mod common;

use common::{
    catalog_has_movie, catalog_has_series, mock_movie, mock_series, start_server, wait_until,
};
use httpmock::prelude::*;
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn initial_scan_indexes_and_streams_movies() {
    let tmdb = MockServer::start();
    mock_movie(&tmdb, "Big Buck Bunny", 1234, "tt1254201", None);

    let temp_media = tempdir().unwrap();
    let video = temp_media.path().join("Big.Buck.Bunny.2008.mp4");
    std::fs::write(&video, "fake video data for Big Buck Bunny").unwrap();

    let base_url = start_server(temp_media.path(), &tmdb.base_url(), None, None).await;
    let client = reqwest::Client::new();

    let in_catalog = wait_until(20, Duration::from_millis(500), || async {
        catalog_has_movie(&client, &base_url, "Big Buck Bunny").await
    })
    .await;
    assert!(in_catalog, "initial movie not found in catalog");

    let resp = client
        .get(format!("{}/stream/movie/tt1254201", base_url))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let stream_resp: serde_json::Value = resp.json().await.unwrap();
    let stream_url = stream_resp["streams"][0]["url"]
        .as_str()
        .expect("Stream URL should be a string")
        .to_string();

    let video_resp = client.get(stream_url).send().await.unwrap();
    assert!(
        video_resp.status().is_success(),
        "video request failed after initial scan"
    );
    assert_eq!(
        video_resp.text().await.unwrap(),
        "fake video data for Big Buck Bunny",
        "video content mismatch"
    );
}

#[tokio::test]
async fn new_file_is_indexed_after_creation() {
    let tmdb = MockServer::start();
    mock_movie(&tmdb, "Sintel", 5678, "tt1727596", None);

    let temp_media = tempdir().unwrap();
    let base_url = start_server(temp_media.path(), &tmdb.base_url(), None, None).await;
    let client = reqwest::Client::new();

    std::fs::write(
        temp_media.path().join("Sintel.2010.mp4"),
        "fake video data for Sintel",
    )
    .unwrap();

    let found = wait_until(20, Duration::from_millis(500), || async {
        catalog_has_movie(&client, &base_url, "Sintel").await
    })
    .await;
    assert!(found, "newly created movie never appeared in catalog");
}

#[tokio::test]
async fn rename_keeps_movie_streamable() {
    let tmdb = MockServer::start();
    mock_movie(&tmdb, "Big Buck Bunny", 1234, "tt1254201", None);
    mock_movie(&tmdb, "Big Buck Bunny Renamed", 1234, "tt1254201", None);

    let temp_media = tempdir().unwrap();
    let original = temp_media.path().join("Big.Buck.Bunny.2008.mp4");
    std::fs::write(&original, "fake video data for Big Buck Bunny").unwrap();

    let base_url = start_server(temp_media.path(), &tmdb.base_url(), None, None).await;
    let client = reqwest::Client::new();

    let renamed = temp_media.path().join("Big.Buck.Bunny.Renamed.mp4");
    std::fs::rename(&original, &renamed).unwrap();

    let streamable = wait_until(20, Duration::from_millis(500), || async {
        let resp = client
            .get(format!("{}/stream/movie/tt1254201", base_url))
            .send()
            .await;
        if let Ok(r) = resp {
            if let Ok(stream_resp) = r.json::<serde_json::Value>().await {
                if let Some(stream_url) = stream_resp["streams"][0]["url"].as_str() {
                    return matches!(
                        client.get(stream_url).send().await,
                        Ok(vr) if vr.status().is_success()
                    );
                }
            }
        }
        false
    })
    .await;
    assert!(streamable, "video not streamable after rename");
}

#[tokio::test]
async fn catalog_includes_tmdb_metadata() {
    let tmdb = MockServer::start();
    mock_movie(&tmdb, "Big Buck Bunny", 1234, "tt1254201", None);

    let temp_media = tempdir().unwrap();
    std::fs::write(
        temp_media.path().join("Big.Buck.Bunny.2008.mp4"),
        "fake video data for Big Buck Bunny",
    )
    .unwrap();

    let base_url = start_server(temp_media.path(), &tmdb.base_url(), None, None).await;
    let client = reqwest::Client::new();

    let indexed = wait_until(20, Duration::from_millis(500), || async {
        catalog_has_movie(&client, &base_url, "Big Buck Bunny").await
    })
    .await;
    assert!(indexed, "movie never indexed");

    let resp = client
        .get(format!("{}/catalog/movie/lanio-movies", base_url))
        .send()
        .await
        .unwrap();
    let json: serde_json::Value = resp.json().await.unwrap();
    let meta = json["metas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == "tt1254201")
        .expect("movie meta not in catalog");

    assert_eq!(meta["name"], "Big Buck Bunny");
    assert_eq!(meta["releaseInfo"], "2008");
    assert_eq!(meta["description"], "A thrilling test overview.");
    assert_eq!(meta["poster"], "https://image.tmdb.org/t/p/w500/poster.jpg");
    assert_eq!(meta["posterShape"], "poster");
    assert_eq!(meta["appExtras"]["ratings"]["tmdb"]["rating"], "8.1");
    assert_eq!(meta["appExtras"]["ratings"]["tmdb"]["votes"], 1200);
    assert!(meta.get("genres").is_none());
    assert!(meta.get("background").is_none());
}

#[tokio::test]
async fn removed_file_disappears_from_catalog() {
    let tmdb = MockServer::start();
    mock_movie(&tmdb, "Sintel", 5678, "tt1727596", None);

    let temp_media = tempdir().unwrap();
    let sintel = temp_media.path().join("Sintel.2010.mp4");
    std::fs::write(&sintel, "fake video data for Sintel").unwrap();

    let base_url = start_server(temp_media.path(), &tmdb.base_url(), None, None).await;
    let client = reqwest::Client::new();

    let present = wait_until(20, Duration::from_millis(500), || async {
        catalog_has_movie(&client, &base_url, "Sintel").await
    })
    .await;
    assert!(present, "Sintel never indexed");

    std::fs::remove_file(&sintel).unwrap();

    let gone = wait_until(20, Duration::from_millis(500), || async {
        !catalog_has_movie(&client, &base_url, "Sintel").await
    })
    .await;
    assert!(gone, "removed movie still present in catalog");
}

/// The season folder sits between the file and the show folder, so the year
/// and the show must still be resolved.
#[tokio::test]
async fn season_folder_layout_indexes_and_streams_episode() {
    let tmdb = MockServer::start();
    mock_series(&tmdb, "Breaking Bad", Some("2008"), 1396, "tt0903747");

    let temp_media = tempdir().unwrap();
    let episode_dir = temp_media
        .path()
        .join("Breaking Bad (2008)")
        .join("Season 01");
    std::fs::create_dir_all(&episode_dir).unwrap();
    std::fs::write(
        episode_dir.join("Breaking.Bad.S01E01.mkv"),
        "fake video data for Breaking Bad S01E01",
    )
    .unwrap();

    let base_url = start_server(temp_media.path(), &tmdb.base_url(), None, None).await;
    let client = reqwest::Client::new();

    let in_catalog = wait_until(20, Duration::from_millis(500), || async {
        catalog_has_series(&client, &base_url, "Breaking Bad").await
    })
    .await;
    assert!(in_catalog, "series in a season folder not found in catalog");

    let resp = client
        .get(format!("{}/stream/series/tt0903747:1:1", base_url))
        .send()
        .await
        .unwrap();
    let stream_resp: serde_json::Value = resp.json().await.unwrap();
    let stream_url = stream_resp["streams"][0]["url"]
        .as_str()
        .expect("expected a stream for S01E01")
        .to_string();

    let video_resp = client.get(stream_url).send().await.unwrap();
    assert!(
        video_resp.status().is_success(),
        "video request failed for a file inside a season folder"
    );
    assert_eq!(
        video_resp.text().await.unwrap(),
        "fake video data for Breaking Bad S01E01",
    );
}

/// `Season 01/01 - Pilot.mkv` has no SxxEyy, so the season comes from the
/// folder and the episode from the filename's leading number.
#[tokio::test]
async fn leading_episode_number_inside_season_folder_streams() {
    let tmdb = MockServer::start();
    mock_series(&tmdb, "Breaking Bad", Some("2008"), 1396, "tt0903747");

    let temp_media = tempdir().unwrap();
    let episode_dir = temp_media
        .path()
        .join("Breaking Bad (2008)")
        .join("Season 01");
    std::fs::create_dir_all(&episode_dir).unwrap();
    std::fs::write(
        episode_dir.join("01 - Pilot.mkv"),
        "fake video data for the Breaking Bad pilot",
    )
    .unwrap();

    let base_url = start_server(temp_media.path(), &tmdb.base_url(), None, None).await;
    let client = reqwest::Client::new();

    let in_catalog = wait_until(20, Duration::from_millis(500), || async {
        catalog_has_series(&client, &base_url, "Breaking Bad").await
    })
    .await;
    assert!(in_catalog, "series in a season folder not found in catalog");

    let resp = client
        .get(format!("{}/stream/series/tt0903747:1:1", base_url))
        .send()
        .await
        .unwrap();
    let stream_resp: serde_json::Value = resp.json().await.unwrap();
    let stream_url = stream_resp["streams"][0]["url"]
        .as_str()
        .expect("a leading episode number must resolve to a streamable episode")
        .to_string();

    let video_resp = client.get(stream_url).send().await.unwrap();
    assert!(video_resp.status().is_success());
    assert_eq!(
        video_resp.text().await.unwrap(),
        "fake video data for the Breaking Bad pilot",
    );
}

/// `S01/Pilot.01.mkv` — the number trails the title, and a bare `S01` folder
/// supplies the season.
#[tokio::test]
async fn trailing_episode_number_in_bare_season_folder_streams() {
    let tmdb = MockServer::start();
    mock_series(&tmdb, "Breaking Bad", Some("2008"), 1396, "tt0903747");

    let temp_media = tempdir().unwrap();
    let episode_dir = temp_media.path().join("Breaking Bad (2008)").join("S01");
    std::fs::create_dir_all(&episode_dir).unwrap();
    std::fs::write(
        episode_dir.join("Pilot.01.mkv"),
        "fake video data for the Breaking Bad pilot",
    )
    .unwrap();

    let base_url = start_server(temp_media.path(), &tmdb.base_url(), None, None).await;
    let client = reqwest::Client::new();

    let in_catalog = wait_until(20, Duration::from_millis(500), || async {
        catalog_has_series(&client, &base_url, "Breaking Bad").await
    })
    .await;
    assert!(
        in_catalog,
        "series in a bare S01 folder not found in catalog"
    );

    let resp = client
        .get(format!("{}/stream/series/tt0903747:1:1", base_url))
        .send()
        .await
        .unwrap();
    let stream_resp: serde_json::Value = resp.json().await.unwrap();
    let stream_url = stream_resp["streams"][0]["url"]
        .as_str()
        .expect("a trailing episode number must resolve to a streamable episode")
        .to_string();

    let video_resp = client.get(stream_url).send().await.unwrap();
    assert!(video_resp.status().is_success());
    assert_eq!(
        video_resp.text().await.unwrap(),
        "fake video data for the Breaking Bad pilot",
    );
}

/// No SxxEyy, but `Specials` still makes it an episode — of season 0.
#[tokio::test]
async fn specials_folder_episode_reaches_the_series_catalog() {
    let tmdb = MockServer::start();
    mock_series(&tmdb, "Breaking Bad", Some("2008"), 1396, "tt0903747");

    let temp_media = tempdir().unwrap();
    let specials = temp_media
        .path()
        .join("Breaking Bad (2008)")
        .join("Specials");
    std::fs::create_dir_all(&specials).unwrap();
    std::fs::write(
        specials.join("El Camino.mkv"),
        "fake video data for the El Camino special",
    )
    .unwrap();

    let base_url = start_server(temp_media.path(), &tmdb.base_url(), None, None).await;
    let client = reqwest::Client::new();

    let in_catalog = wait_until(20, Duration::from_millis(500), || async {
        catalog_has_series(&client, &base_url, "Breaking Bad").await
    })
    .await;
    assert!(
        in_catalog,
        "a Specials folder must be read as season 0 of the show, not a movie \
         named \"Specials\""
    );
}
