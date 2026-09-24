use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileInfo {
    #[serde(rename = "imdbId")]
    pub imdb_id: String,
    pub title: String,
    pub year: Option<u16>,
    #[serde(rename = "type")]
    pub content_type: ContentType,
    #[serde(rename = "filePath")]
    pub file_path: PathBuf,
    pub parsed: ParsedMetadata,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmdb_rating: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmdb_votes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    #[default]
    Movie,
    Series,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ParsedMetadata {
    pub season: Option<u16>,
    pub episode: Option<u16>,
}

#[derive(Debug, Clone)]
pub enum IndexEntry {
    Movie(FileInfo),
    Series(Vec<FileInfo>),
}
