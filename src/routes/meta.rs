use crate::routes::stream::{stream_for_file, Stream, StreamState};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct MetaResponse {
    pub meta: Option<Meta>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub id: String,
    #[serde(rename = "type")]
    pub content_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub videos: Option<Vec<Video>>,
    /// Lets Stremio show the file list instead of a single detail page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_as_videos: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct Video {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streams: Option<Vec<Stream>>,
}

#[derive(Deserialize)]
pub struct MetaPath {
    #[serde(rename = "type")]
    pub content_type: String,
    pub id: String,
}

pub async fn meta_handler(
    Path(MetaPath { content_type, id }): Path<MetaPath>,
    State(state): State<StreamState>,
) -> Json<MetaResponse> {
    Json(meta_inner(content_type, id, &state))
}

fn meta_inner(content_type: String, id: String, state: &StreamState) -> MetaResponse {
    let id = id.strip_suffix(".json").unwrap_or(&id);

    if content_type != "other" || !state.config.unmatched_catalog {
        return MetaResponse { meta: None };
    }

    let Some(file) = state.scanner.index.get_unmatched(id) else {
        return MetaResponse { meta: None };
    };

    let stream = stream_for_file(&file.title, &file.file_path, &state.config);

    MetaResponse {
        meta: Some(Meta {
            id: file.id.clone(),
            content_type: "other".to_string(),
            name: file.title.clone(),
            videos: Some(vec![Video {
                id: file.id,
                title: file.title,
                streams: Some(vec![stream]),
            }]),
            show_as_videos: Some(true),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::index::types::UnmatchedFile;
    use crate::index::MediaIndex;
    use crate::metadata::TmdbClient;
    use crate::scanner::MediaScanner;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn make_state(unmatched_catalog: bool) -> StreamState {
        let config = Arc::new(Config {
            media_path: PathBuf::from("/media"),
            port: 8078,
            base_url: Some("http://lan.local:8078".to_string()),
            public_url: None,
            tmdb_api_key: "fake".to_string(),
            tmdb_base_url: "http://localhost".to_string(),
            tmdb_image_base_url: "http://localhost".to_string(),
            password: None,
            auth_token: None,
            poster_url: None,
            scan_cron: None,
            unmatched_catalog,
        });

        let index = Arc::new(MediaIndex::new());
        let tmdb_client = Arc::new(TmdbClient::new(
            "fake".to_string(),
            "http://localhost".to_string(),
            "http://localhost".to_string(),
        ));
        let scanner = Arc::new(MediaScanner::new(index, tmdb_client, Arc::clone(&config)));

        StreamState { scanner, config }
    }

    fn insert_file(state: &StreamState) {
        state.scanner.index.insert_unmatched(UnmatchedFile {
            id: "lanio-abc123".to_string(),
            title: "2024 Brazilian Grand Prix.ts".to_string(),
            file_path: PathBuf::from("/media/2024 Brazilian Grand Prix.ts"),
            created: Some(1000),
        });
    }

    #[tokio::test]
    async fn detail_page_contains_a_playable_video() {
        let state = make_state(true);
        insert_file(&state);

        let response = meta_inner("other".to_string(), "lanio-abc123".to_string(), &state);
        let meta = response.meta.expect("unmatched file must have meta");

        assert_eq!(meta.name, "2024 Brazilian Grand Prix.ts");
        assert_eq!(meta.content_type, "other");
        assert_eq!(meta.show_as_videos, Some(true));

        let json = serde_json::to_value(&meta).unwrap();
        let video = &json["videos"][0];
        assert_eq!(video["id"], "lanio-abc123");
        assert_eq!(video["title"], "2024 Brazilian Grand Prix.ts");
        assert!(video["streams"][0]["url"]
            .as_str()
            .unwrap()
            .starts_with("http://lan.local:8078/video?path="));
    }

    #[tokio::test]
    async fn detail_page_is_empty_when_disabled() {
        let state = make_state(false);
        insert_file(&state);

        let response = meta_inner("other".to_string(), "lanio-abc123".to_string(), &state);
        assert!(response.meta.is_none());
    }

    #[tokio::test]
    async fn unknown_file_has_no_meta() {
        let state = make_state(true);
        let response = meta_inner("other".to_string(), "lanio-missing".to_string(), &state);
        assert!(response.meta.is_none());
    }
}
