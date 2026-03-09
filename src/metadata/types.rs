use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct MediaMetadata {
    pub imdb_id: String,
    pub poster_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TmdbSearchResponse {
    pub results: Vec<TmdbSearchResult>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TmdbSearchResult {
    pub id: u64,
    pub poster_path: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TmdbMovieDetails {
    pub imdb_id: Option<String>,
    pub poster_path: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TmdbTvDetails {
    pub poster_path: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TmdbTvExternalIds {
    pub imdb_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TmdbFindResponse {
    pub movie_results: Vec<TmdbFindResult>,
    pub tv_results: Vec<TmdbFindResult>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TmdbFindResult {
    pub poster_path: Option<String>,
}
