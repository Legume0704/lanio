use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::index::types::FileInfo;
use crate::scanner::MediaScanner;

#[derive(Debug, Serialize)]
pub struct CatalogResponse {
    pub metas: Vec<Meta>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub id: String,
    #[serde(rename = "type")]
    pub content_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
    pub poster_shape: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_info: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_extras: Option<AppExtras>,
}

#[derive(Debug, Serialize)]
pub struct AppExtras {
    pub ratings: Ratings,
}

#[derive(Debug, Serialize)]
pub struct Ratings {
    pub tmdb: TmdbRating,
}

#[derive(Debug, Serialize)]
pub struct TmdbRating {
    pub rating: String,
    pub votes: u32,
}

fn meta_from_file_info(
    imdb_id: String,
    content_type: &str,
    file_info: &FileInfo,
    poster: Option<String>,
) -> Meta {
    let release_info = file_info.year.map(|y| format!("{}", y));
    let app_extras = file_info.tmdb_rating.map(|rating| AppExtras {
        ratings: Ratings {
            tmdb: TmdbRating {
                rating: format!("{:.1}", rating),
                votes: file_info.tmdb_votes.unwrap_or(0),
            },
        },
    });

    Meta {
        id: imdb_id,
        content_type: content_type.to_string(),
        name: file_info.title.clone(),
        poster,
        poster_shape: "poster".to_string(),
        release_info,
        description: file_info.description.clone(),
        app_extras,
    }
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
    Path(CatalogPath {
        content_type,
        id: catalog_id,
    }): Path<CatalogPath>,
    State(state): State<CatalogState>,
) -> Json<CatalogResponse> {
    Json(catalog_inner(content_type, catalog_id, &state))
}

fn catalog_inner(
    content_type: String,
    catalog_id: String,
    state: &CatalogState,
) -> CatalogResponse {
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
                    .or(file_info.poster.clone());
                meta_from_file_info(imdb_id, "movie", &file_info, poster)
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
                    .or(file_info.poster.clone());
                meta_from_file_info(imdb_id, "series", &file_info, poster)
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
            scan_cron: None,
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
                ..Default::default()
            },
        );

        let res = catalog_inner("movie".to_string(), "lanio-movies".to_string(), &state);
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
                ..Default::default()
            },
        );

        let res = catalog_inner("movie".to_string(), "lanio-movies".to_string(), &state);
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
                ..Default::default()
            },
        );

        let res = catalog_inner("series".to_string(), "lanio-series".to_string(), &state);
        assert_eq!(res.metas.len(), 1);
        assert_eq!(
            res.metas[0].poster.as_deref(),
            Some("https://btttr.cc/poster/imdb/poster-default/tt7654321.jpg")
        );
    }

    #[tokio::test]
    async fn catalog_includes_tmdb_metadata_fields() {
        let state = make_test_state(None);
        state.scanner.index.insert_movie(
            "tt1234567".to_string(),
            FileInfo {
                imdb_id: "tt1234567".to_string(),
                title: "Inception".to_string(),
                year: Some(2010),
                content_type: ContentType::Movie,
                file_path: PathBuf::from("/media/Inception.mkv"),
                parsed: ParsedMetadata {
                    season: None,
                    episode: None,
                },
                poster: None,
                description: Some("A thief who steals corporate secrets.".to_string()),
                tmdb_rating: Some(8.37),
                tmdb_votes: Some(30000),
            },
        );

        let res = catalog_inner("movie".to_string(), "lanio-movies".to_string(), &state);
        let meta = &res.metas[0];
        let json = serde_json::to_value(meta).unwrap();

        assert_eq!(meta.name, "Inception");
        assert_eq!(json["posterShape"], "poster");
        assert_eq!(json["releaseInfo"], "2010");
        assert_eq!(json["description"], "A thief who steals corporate secrets.");
        assert_eq!(json["appExtras"]["ratings"]["tmdb"]["rating"], "8.4");
        assert_eq!(json["appExtras"]["ratings"]["tmdb"]["votes"], 30000);
    }

    #[tokio::test]
    async fn catalog_omits_missing_metadata_fields() {
        let state = make_test_state(None);
        state.scanner.index.insert_movie(
            "tt1234567".to_string(),
            FileInfo {
                imdb_id: "tt1234567".to_string(),
                title: "Mystery Movie".to_string(),
                year: None,
                content_type: ContentType::Movie,
                file_path: PathBuf::from("/media/Mystery Movie.mkv"),
                parsed: ParsedMetadata {
                    season: None,
                    episode: None,
                },
                ..Default::default()
            },
        );

        let res = catalog_inner("movie".to_string(), "lanio-movies".to_string(), &state);
        let json = serde_json::to_value(&res.metas[0]).unwrap();

        assert!(json.get("releaseInfo").is_none());
        assert!(json.get("description").is_none());
        assert!(json.get("appExtras").is_none());
    }
}
