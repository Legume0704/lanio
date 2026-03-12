use crate::config::Config;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub resources: Vec<String>,
    pub types: Vec<String>,
    pub id_prefixes: Vec<String>,
    pub catalogs: Vec<Catalog>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub id: String,
    pub name: String,
    pub r#type: String,
}

fn manifest_data() -> Manifest {
    Manifest {
        id: "lanio.local.com".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        name: "Lanio".to_string(),
        description: "Stream media files over local network".to_string(),
        resources: vec!["catalog".to_string(), "stream".to_string()],
        types: vec!["movie".to_string(), "series".to_string()],
        id_prefixes: vec!["tt".to_string()],
        catalogs: vec![
            Catalog {
                id: "lanio-movies".to_string(),
                name: "Lanio".to_string(),
                r#type: "movie".to_string(),
            },
            Catalog {
                id: "lanio-series".to_string(),
                name: "Lanio".to_string(),
                r#type: "series".to_string(),
            },
        ],
    }
}

/// No-auth manifest handler (used when PASSWORD is not configured).
pub async fn get_manifest() -> Json<Manifest> {
    Json(manifest_data())
}

/// Token-validated manifest handler (used when PASSWORD is configured).
pub async fn get_manifest_authed(
    Path(token): Path<String>,
    State(config): State<Arc<Config>>,
) -> Result<Json<Manifest>, StatusCode> {
    if !config.is_valid_token(&token) {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(manifest_data()))
}
