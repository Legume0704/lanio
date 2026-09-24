use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct MediaMetadata {
    pub imdb_id: String,
    pub title: Option<String>,
    pub year: Option<u16>,
    pub overview: Option<String>,
    pub poster_url: Option<String>,
    pub tmdb_rating: Option<f64>,
    pub tmdb_votes: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TmdbSearchResponse {
    pub results: Vec<TmdbSearchResult>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TmdbSearchResult {
    pub id: u64,
    #[serde(alias = "name")]
    pub title: Option<String>,
    pub poster_path: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TmdbMovieDetails {
    pub imdb_id: Option<String>,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub release_date: Option<String>,
    pub poster_path: Option<String>,
    pub vote_average: Option<f64>,
    pub vote_count: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TmdbTvDetails {
    #[serde(alias = "title")]
    pub name: Option<String>,
    pub overview: Option<String>,
    pub first_air_date: Option<String>,
    pub poster_path: Option<String>,
    pub vote_average: Option<f64>,
    pub vote_count: Option<u32>,
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
    #[serde(alias = "name")]
    pub title: Option<String>,
    pub overview: Option<String>,
    pub release_date: Option<String>,
    pub first_air_date: Option<String>,
    pub poster_path: Option<String>,
    pub vote_average: Option<f64>,
    pub vote_count: Option<u32>,
}
