use crate::scanner::MediaScanner;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RescanResponse {
    pub status: String,
    pub message: String,
}

pub async fn rescan_handler(State(scanner): State<Arc<MediaScanner>>) -> Response {
    let scanner_clone = Arc::clone(&scanner);
    tokio::spawn(async move {
        if let Err(e) = scanner_clone.scan().await {
            tracing::error!("Rescan failed: {}", e);
        }
    });

    (
        StatusCode::ACCEPTED,
        Json(RescanResponse {
            status: "ok".to_string(),
            message: "Media library rescan requested".to_string(),
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::index::MediaIndex;
    use crate::metadata::TmdbClient;
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;

    fn make_test_scanner() -> Arc<MediaScanner> {
        let config = Arc::new(Config {
            media_path: PathBuf::from("/media"),
            port: 8078,
            base_url: None,
            public_url: None,
            tmdb_api_key: "fake".to_string(),
            tmdb_base_url: "http://localhost".to_string(),
            tmdb_image_base_url: "http://localhost".to_string(),
            password: None,
            auth_token: None,
            poster_url: None,
            scan_cron: None,
        });

        let index = Arc::new(MediaIndex::new());
        let tmdb_client = Arc::new(TmdbClient::new(
            "fake".to_string(),
            "http://localhost".to_string(),
            "http://localhost".to_string(),
        ));
        Arc::new(MediaScanner::new(index, tmdb_client, config))
    }

    #[tokio::test]
    async fn rescan_returns_202_when_not_scanning() {
        let scanner = make_test_scanner();
        assert!(!scanner.scanning.load(Ordering::SeqCst));

        let res = rescan_handler(State(scanner)).await;
        assert_eq!(res.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn rescan_returns_202_when_already_scanning() {
        let scanner = make_test_scanner();
        scanner.scanning.store(true, Ordering::SeqCst);

        let res = rescan_handler(State(scanner.clone())).await;
        assert_eq!(res.status(), StatusCode::ACCEPTED);
        scanner.scanning.store(false, Ordering::SeqCst);
        scanner.pending_rescan.store(false, Ordering::SeqCst);
    }
}
