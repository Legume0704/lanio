mod common;

use common::{
    catalog_has_movie, health_scanning, mock_movie, start_server, stream_count, token_for,
    wait_until,
};
use httpmock::prelude::*;
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn rescan_endpoint_triggers_scan() {
    let tmdb = MockServer::start();
    // A slow details response keeps the scan "scanning" long enough to observe
    // via /health. Only scan() (triggered here) sets the flag — the file
    // watcher never does.
    mock_movie(
        &tmdb,
        "Slow",
        800,
        "tt0000800",
        Some(Duration::from_secs(5)),
    );

    let temp_media = tempdir().unwrap();
    // Start with an empty library so the initial scan is instant and does not
    // populate the TMDB cache for this movie.
    let base_url = start_server(temp_media.path(), &tmdb.base_url(), None, None).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{}/rescan", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 202, "rescan should be accepted");
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "ok");

    // Drop a file in, then rescape — its (uncached) metadata lookup keeps the
    // scan slow so the /health flag is observable.
    std::fs::write(temp_media.path().join("Slow.2020.mp4"), "slow data").unwrap();
    let res = client
        .post(format!("{}/rescan", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 202);

    let scan_started = wait_until(40, Duration::from_millis(100), || async {
        health_scanning(&client, &base_url).await == Some(true)
    })
    .await;
    assert!(scan_started, "rescan endpoint did not start a scan");

    let indexed = wait_until(60, Duration::from_millis(500), || async {
        stream_count(&client, &base_url, "tt0000800").await == Some(1)
    })
    .await;
    assert!(indexed, "movie never indexed after rescan");
}

#[tokio::test]
async fn rescan_endpoint_returns_202() {
    let tmdb = MockServer::start();
    mock_movie(&tmdb, "Alpha", 100, "tt0000100", None);
    mock_movie(&tmdb, "Bravo", 200, "tt0000200", None);

    let temp_media = tempdir().unwrap();
    std::fs::write(temp_media.path().join("Alpha.2020.mp4"), "alpha data").unwrap();

    let base_url = start_server(temp_media.path(), &tmdb.base_url(), None, None).await;
    let client = reqwest::Client::new();

    let alpha_found = wait_until(20, Duration::from_millis(500), || async {
        catalog_has_movie(&client, &base_url, "Alpha").await
    })
    .await;
    assert!(alpha_found, "Alpha never indexed by initial scan");

    // Fire several rescans concurrently — overlapping requests still queue cleanly.
    let handles: Vec<_> = (0..5)
        .map(|_| {
            let client = client.clone();
            let url = base_url.clone();
            tokio::spawn(
                async move { client.post(format!("{}/rescan", url)).send().await.unwrap() },
            )
        })
        .collect();

    for handle in handles {
        let res = handle.await.unwrap();
        assert_eq!(res.status(), 202, "rescan should be accepted");
        let body: serde_json::Value = res.json().await.unwrap();
        assert_eq!(body["status"], "ok");
    }

    // New media dropped into the library and a rescan requested — it should show up.
    std::fs::write(temp_media.path().join("Bravo.2020.mp4"), "bravo data").unwrap();
    let res = client
        .post(format!("{}/rescan", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 202);

    let bravo_found = wait_until(20, Duration::from_millis(500), || async {
        catalog_has_movie(&client, &base_url, "Bravo").await
    })
    .await;
    assert!(bravo_found, "Bravo never appeared in catalog after rescan");
}

#[tokio::test]
async fn rescan_route_requires_valid_token() {
    let tmdb = MockServer::start();
    mock_movie(&tmdb, "Alpha", 100, "tt0000100", None);

    let temp_media = tempdir().unwrap();
    std::fs::write(temp_media.path().join("Alpha.2020.mp4"), "alpha data").unwrap();

    let base_url = start_server(temp_media.path(), &tmdb.base_url(), Some("hunter2"), None).await;
    let client = reqwest::Client::new();
    let token = token_for("hunter2");

    let res = client
        .post(format!("{}/rescan", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404, "rescan should require a token");

    let res = client
        .post(format!("{}/{}/rescan", base_url, "wrong-token"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404, "wrong token should be rejected");

    let res = client
        .post(format!("{}/{}/rescan", base_url, token))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 202, "valid token should be accepted");
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn cron_scheduled_scan_runs() {
    let tmdb = MockServer::start();
    // The slow details response keeps the scheduled scan "scanning" long enough
    // to observe via /health. We never POST /rescan, so an observed scan can
    // only come from the cron scheduler.
    mock_movie(
        &tmdb,
        "Slow",
        800,
        "tt0000800",
        Some(Duration::from_secs(5)),
    );

    let temp_media = tempdir().unwrap();
    // Start with an empty library so the initial scan is instant.
    let base_url = start_server(
        temp_media.path(),
        &tmdb.base_url(),
        None,
        Some("* * * * * *"),
    )
    .await;
    let client = reqwest::Client::new();

    // Drop a file in after startup; the next scheduled tick should scan it.
    std::fs::write(temp_media.path().join("Slow.2020.mp4"), "slow data").unwrap();

    let scan_started = wait_until(40, Duration::from_millis(100), || async {
        health_scanning(&client, &base_url).await == Some(true)
    })
    .await;
    assert!(scan_started, "cron schedule did not trigger a scan");

    let indexed = wait_until(60, Duration::from_millis(500), || async {
        stream_count(&client, &base_url, "tt0000800").await == Some(1)
    })
    .await;
    assert!(indexed, "movie never indexed by scheduled scan");
}
