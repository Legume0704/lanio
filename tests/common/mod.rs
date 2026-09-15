// The helper module exports more than any single test binary uses, and the
// env lock is deliberately held across awaits while a server boots.
#![allow(dead_code, clippy::await_holding_lock)]
//! Shared test infrastructure for integration tests.
//!
//! Helper: `mod common;` from each `tests/*.rs` file (each integration test
//! file is its own crate, so this module is compiled once per binary).

use httpmock::prelude::*;
use serde_json::json;
use sha2::{Digest, Sha512};
use std::future::Future;
use std::sync::Mutex;
use std::time::Duration;
use tokio::net::TcpListener;

// The env lock keeps tests within a single test binary from clobbering each
// other's process-global env vars while each boots a separate server. The app
// reads all config from env during startup, so the lock only needs to be held
// until the health check confirms startup completed.
static ENV_LOCK: Mutex<()> = Mutex::new(());

pub async fn wait_until<F, Fut>(max_retries: usize, interval: Duration, mut f: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    for _ in 0..max_retries {
        if f().await {
            return true;
        }
        tokio::time::sleep(interval).await;
    }
    false
}

pub fn token_for(password: &str) -> String {
    let input = format!("lanio_auth:{}", password);
    let hash = Sha512::digest(input.as_bytes());
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn mock_movie(
    server: &MockServer,
    query: &str,
    id: u32,
    imdb_id: &str,
    details_delay: Option<Duration>,
) {
    server.mock(|when, then| {
        when.method(GET)
            .path("/search/movie")
            .query_param("query", query);
        then.status(200)
            .json_body(json!({ "results": [{ "id": id }] }));
    });
    server.mock(|when, then| {
        when.method(GET).path(format!("/movie/{}", id));
        let then = then
            .status(200)
            .json_body(json!({ "imdb_id": imdb_id, "poster_path": "/poster.jpg" }));
        match details_delay {
            Some(delay) => then.delay(delay),
            None => then,
        };
    });
}

pub async fn start_server(
    media_path: &std::path::Path,
    tmdb_base_url: &str,
    password: Option<&str>,
    scan_cron: Option<&str>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    std::env::set_var("MEDIA_PATH", media_path);
    std::env::set_var("TMDB_BASE_URL", tmdb_base_url);
    std::env::set_var("TMDB_API_KEY", "fake_key");
    std::env::set_var("PORT", addr.port().to_string());
    std::env::set_var("BASE_URL", &base_url);
    match password {
        Some(p) => std::env::set_var("PASSWORD", p),
        None => std::env::remove_var("PASSWORD"),
    }
    match scan_cron {
        Some(c) => std::env::set_var("SCAN_CRON", c),
        None => std::env::remove_var("SCAN_CRON"),
    }

    tokio::spawn(async move {
        lanio::run(Some(listener)).await.expect("App failed to run");
    });

    let client = reqwest::Client::new();
    let ready = wait_until(20, Duration::from_millis(100), || async {
        let resp = client.get(format!("{}/health", base_url)).send().await;
        matches!(resp, Ok(r) if r.status().is_success())
    })
    .await;
    assert!(ready, "App never became ready");

    base_url
}

pub async fn catalog_has_movie(client: &reqwest::Client, base_url: &str, name: &str) -> bool {
    let resp = client
        .get(format!("{}/catalog/movie/lanio-movies", base_url))
        .send()
        .await;
    if let Ok(r) = resp {
        if let Ok(json) = r.json::<serde_json::Value>().await {
            return json["metas"]
                .as_array()
                .map(|m| m.iter().any(|x| x["name"] == name))
                .unwrap_or(false);
        }
    }
    false
}

pub async fn stream_count(
    client: &reqwest::Client,
    base_url: &str,
    imdb_id: &str,
) -> Option<usize> {
    let resp = client
        .get(format!("{}/stream/movie/{}", base_url, imdb_id))
        .send()
        .await
        .ok()?;
    let json: serde_json::Value = resp.json().await.ok()?;
    json["streams"].as_array().map(|s| s.len())
}

pub async fn health_scanning(client: &reqwest::Client, base_url: &str) -> Option<bool> {
    let resp = client
        .get(format!("{}/health", base_url))
        .send()
        .await
        .ok()?;
    let json: serde_json::Value = resp.json().await.ok()?;
    json["scanning"].as_bool()
}
