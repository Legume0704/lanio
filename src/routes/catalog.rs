use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use std::sync::Arc;

use crate::config::Config;
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
    pub config: Arc<Config>,
}

/// No-auth catalog handler (used when PASSWORD is not configured).
pub async fn catalog_handler(
    Path((content_type, catalog_id)): Path<(String, String)>,
    State(state): State<CatalogState>,
) -> Json<CatalogResponse> {
    Json(catalog_inner(content_type, catalog_id, &state))
}

/// Token-validated catalog handler (used when PASSWORD is configured).
pub async fn catalog_handler_authed(
    Path((token, content_type, catalog_id)): Path<(String, String, String)>,
    State(state): State<CatalogState>,
) -> Result<Json<CatalogResponse>, StatusCode> {
    if !state.config.is_valid_token(&token) {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(catalog_inner(content_type, catalog_id, &state)))
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
            .map(|(imdb_id, file_info)| Meta {
                id: imdb_id,
                content_type: "movie".to_string(),
                name: file_info.title,
                poster: file_info.poster,
            })
            .collect(),
        "series" if catalog_id == "lanio-series" => state
            .scanner
            .index
            .get_all_series()
            .into_iter()
            .map(|(imdb_id, file_info)| Meta {
                id: imdb_id,
                content_type: "series".to_string(),
                name: file_info.title,
                poster: file_info.poster,
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
