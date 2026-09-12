use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::scanner::MediaScanner;

#[derive(Debug, Serialize)]
pub struct CatalogResponse {
    pub metas: Vec<Meta>,
}

#[derive(Debug, Serialize)]
pub struct Meta {
    pub id: String,
    #[serde(rename = "type")]
    pub content_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
}

#[derive(Clone)]
pub struct CatalogState {
    pub scanner: Arc<MediaScanner>,
}

#[derive(Deserialize)]
pub struct CatalogPath {
    #[serde(rename = "type")]
    pub content_type: String,
    pub id: String,
}

pub async fn catalog_handler(
    Path(CatalogPath { content_type, id: catalog_id }): Path<CatalogPath>,
    State(state): State<CatalogState>,
) -> Json<CatalogResponse> {
    Json(catalog_inner(content_type, catalog_id, &state))
}

fn catalog_inner(content_type: String, catalog_id: String, state: &CatalogState) -> CatalogResponse {
    // Strip .json extension if present
    let catalog_id = catalog_id.strip_suffix(".json").unwrap_or(&catalog_id);

    tracing::debug!("Catalog request: type={}, id={}", content_type, catalog_id);

    let metas = match content_type.as_str() {
        "movie" if catalog_id == "lanio-movies" => state
            .scanner
            .index
            .get_all_movies()
            .into_iter()
            .map(|(imdb_id, file_info)| {
                let poster = state
                    .scanner
                    .config
                    .poster_url_for(&imdb_id)
                    .or(file_info.poster);
                Meta {
                    id: imdb_id,
                    content_type: "movie".to_string(),
                    name: file_info.title,
                    poster,
                }
            })
            .collect(),
        "series" if catalog_id == "lanio-series" => state
            .scanner
            .index
            .get_all_series()
            .into_iter()
            .map(|(imdb_id, file_info)| {
                let poster = state
                    .scanner
                    .config
                    .poster_url_for(&imdb_id)
                    .or(file_info.poster);
                Meta {
                    id: imdb_id,
                    content_type: "series".to_string(),
                    name: file_info.title,
                    poster,
                }
            })
            .collect(),
        _ => {
            tracing::warn!("Invalid catalog request: {}/{}", content_type, catalog_id);
            vec![]
        }
    };

    tracing::debug!("Returning {} items", metas.len());

    CatalogResponse { metas }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::index::types::{ContentType, FileInfo, ParsedMetadata};
    use crate::index::MediaIndex;
    use crate::metadata::TmdbClient;
    use std::path::PathBuf;

    fn make_test_state(poster_url: Option<String>) -> CatalogState {
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
            poster_url,
        });

        let index = Arc::new(MediaIndex::new());
        let tmdb_client = Arc::new(TmdbClient::new(
            "fake".to_string(),
            "http://localhost".to_string(),
            "http://localhost".to_string(),
        ));
        let scanner = Arc::new(MediaScanner::new(index, tmdb_client, config));

        CatalogState { scanner }
    }

    #[tokio::test]
    async fn catalog_uses_file_info_poster_when_no_poster_url() {
        let state = make_test_state(None);
        state.scanner.index.insert_movie(
            "tt1234567".to_string(),
            FileInfo {
                imdb_id: "tt1234567".to_string(),
                title: "Test Movie".to_string(),
                year: Some(2024),
                content_type: ContentType::Movie,
                file_path: PathBuf::from("/media/Test Movie.mp4"),
                parsed: ParsedMetadata {
                    season: None,
                    episode: None,
                },
                poster: Some("https://image.tmdb.org/t/p/w500/test.jpg".to_string()),
            },
        );

        let res = catalog_inner(
            "movie".to_string(),
            "lanio-movies".to_string(),
            &state,
        );
        assert_eq!(res.metas.len(), 1);
        assert_eq!(
            res.metas[0].poster.as_deref(),
            Some("https://image.tmdb.org/t/p/w500/test.jpg")
        );
    }

    #[tokio::test]
    async fn catalog_overrides_poster_when_poster_url_configured() {
        let state = make_test_state(Some(
            "https://btttr.cc/poster/imdb/poster-default/{imdb_id}.jpg".to_string(),
        ));
        state.scanner.index.insert_movie(
            "tt1234567".to_string(),
            FileInfo {
                imdb_id: "tt1234567".to_string(),
                title: "Test Movie".to_string(),
                year: Some(2024),
                content_type: ContentType::Movie,
                file_path: PathBuf::from("/media/Test Movie.mp4"),
                parsed: ParsedMetadata {
                    season: None,
                    episode: None,
                },
                poster: Some("https://image.tmdb.org/t/p/w500/test.jpg".to_string()),
            },
        );

        let res = catalog_inner(
            "movie".to_string(),
            "lanio-movies".to_string(),
            &state,
        );
        assert_eq!(res.metas.len(), 1);
        assert_eq!(
            res.metas[0].poster.as_deref(),
            Some("https://btttr.cc/poster/imdb/poster-default/tt1234567.jpg")
        );
    }

    #[tokio::test]
    async fn catalog_series_uses_poster_url_when_configured() {
        let state = make_test_state(Some(
            "https://btttr.cc/poster/imdb/poster-default/{imdb_id}.jpg".to_string(),
        ));
        state.scanner.index.insert_episode(
            "tt7654321".to_string(),
            FileInfo {
                imdb_id: "tt7654321".to_string(),
                title: "Test Series".to_string(),
                year: Some(2024),
                content_type: ContentType::Series,
                file_path: PathBuf::from("/media/Test Series S01E01.mp4"),
                parsed: ParsedMetadata {
                    season: Some(1),
                    episode: Some(1),
                },
                poster: None,
            },
        );

        let res = catalog_inner(
            "series".to_string(),
            "lanio-series".to_string(),
            &state,
        );
        assert_eq!(res.metas.len(), 1);
        assert_eq!(
            res.metas[0].poster.as_deref(),
            Some("https://btttr.cc/poster/imdb/poster-default/tt7654321.jpg")
        );
    }
}
