use axum::Json;
use serde::{Deserialize, Serialize};

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

pub async fn get_manifest() -> Json<Manifest> {
    Json(Manifest {
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
    })
}
