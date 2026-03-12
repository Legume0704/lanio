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
    content_type: String,
    id: String,
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
