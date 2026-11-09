use crate::config::Config;
use crate::routes::catalog::UNMATCHED_CATALOG_ID;
use crate::scanner::UNMATCHED_ID_PREFIX;
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub resources: Vec<Resource>,
    pub types: Vec<String>,
    pub id_prefixes: Vec<String>,
    pub catalogs: Vec<Catalog>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Resource {
    Name(String),
    Detailed {
        name: String,
        types: Vec<String>,
        #[serde(rename = "idPrefixes", skip_serializing_if = "Option::is_none")]
        id_prefixes: Option<Vec<String>>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub id: String,
    pub name: String,
    pub r#type: String,
}

fn manifest_data(config: &Config) -> Manifest {
    let mut types = vec!["movie".to_string(), "series".to_string()];
    let mut id_prefixes = vec!["tt".to_string()];
    let mut resources = vec![
        Resource::Name("catalog".to_string()),
        Resource::Name("stream".to_string()),
    ];
    let mut catalogs = vec![
        Catalog {
            id: "lanio-movies".to_string(),
            name: "Movies on Local Network".to_string(),
            r#type: "movie".to_string(),
        },
        Catalog {
            id: "lanio-series".to_string(),
            name: "Series on Local Network".to_string(),
            r#type: "series".to_string(),
        },
    ];

    if config.unmatched_catalog {
        types.push("other".to_string());
        id_prefixes.push(UNMATCHED_ID_PREFIX.to_string());

        // Only unmatched entries need meta; scoping it keeps the add-on from
        // being asked about every IMDb title Stremio opens.
        resources.push(Resource::Detailed {
            name: "meta".to_string(),
            types: vec!["other".to_string()],
            id_prefixes: Some(vec![UNMATCHED_ID_PREFIX.to_string()]),
        });

        catalogs.push(Catalog {
            id: UNMATCHED_CATALOG_ID.to_string(),
            name: "Unmatched Files on Local Network".to_string(),
            r#type: "other".to_string(),
        });
    }

    Manifest {
        id: "lanio.local.com".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        name: "Lanio".to_string(),
        description: "Stream media files over local network".to_string(),
        resources,
        types,
        id_prefixes,
        catalogs,
    }
}

pub async fn get_manifest(State(config): State<Arc<Config>>) -> Json<Manifest> {
    Json(manifest_data(&config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn config(unmatched_catalog: bool) -> Config {
        Config {
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
            unmatched_catalog,
        }
    }

    #[test]
    fn manifest_omits_unmatched_catalog_by_default() {
        let manifest = manifest_data(&config(false));

        assert_eq!(manifest.types, vec!["movie", "series"]);
        assert_eq!(manifest.id_prefixes, vec!["tt"]);
        assert_eq!(manifest.catalogs.len(), 2);

        let json = serde_json::to_value(&manifest).unwrap();
        assert_eq!(json["resources"][0], "catalog");
        assert_eq!(json["resources"][1], "stream");
    }

    #[test]
    fn manifest_includes_unmatched_catalog_when_enabled() {
        let manifest = manifest_data(&config(true));

        assert_eq!(manifest.types, vec!["movie", "series", "other"]);
        assert_eq!(manifest.id_prefixes, vec!["tt", UNMATCHED_ID_PREFIX]);
        assert_eq!(manifest.catalogs.len(), 3);
        assert_eq!(manifest.catalogs[2].id, UNMATCHED_CATALOG_ID);
        assert_eq!(manifest.catalogs[2].r#type, "other");

        let json = serde_json::to_value(&manifest).unwrap();
        let catalogs = json["catalogs"].as_array().unwrap();
        assert!(catalogs
            .iter()
            .any(|catalog| catalog["id"] == UNMATCHED_CATALOG_ID && catalog["type"] == "other"));

        let meta = json["resources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|resource| resource["name"] == "meta")
            .expect("the unmatched catalog needs a meta resource for its detail page");
        assert_eq!(meta["types"][0], "other");
        assert_eq!(meta["idPrefixes"][0], UNMATCHED_ID_PREFIX);
    }
}
