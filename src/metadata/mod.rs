pub mod rate_limiter;
pub mod types;

use moka::future::Cache;
use rate_limiter::RateLimiter;
use std::time::Duration;
use types::{
    MediaMetadata, TmdbFindResponse, TmdbMovieDetails, TmdbSearchResponse, TmdbTvDetails,
    TmdbTvExternalIds,
};

pub struct TmdbClient {
    client: reqwest::Client,
    api_key: String,
    cache: Cache<String, MediaMetadata>,
    rate_limiter: RateLimiter,
    base_url: String,
    image_base_url: String,
}

impl TmdbClient {
    pub fn new(api_key: String, base_url: String, image_base_url: String) -> Self {
        let cache = Cache::builder()
            .max_capacity(1000)
            .time_to_live(Duration::from_secs(86400)) // 24 hours
            .build();

        Self {
            client: reqwest::Client::new(),
            api_key,
            cache,
            rate_limiter: RateLimiter::new(40), // 40 requests per 10 seconds
            base_url,
            image_base_url,
        }
    }

    pub async fn search_movie(&self, title: &str, year: Option<u16>) -> Option<MediaMetadata> {
        let cache_key = format!("movie:{}:{}", title, year.unwrap_or(0));

        // Check cache first
        if let Some(cached) = self.cache.get(&cache_key).await {
            tracing::debug!("Cache hit for movie: {}", title);
            return Some(cached);
        }

        // Rate limit
        let _permit = self.rate_limiter.acquire().await;

        // Search for movie
        let mut url = format!("{}/search/movie", self.base_url);
        let mut params = vec![
            ("api_key", self.api_key.clone()),
            ("query", title.to_string()),
        ];
        if let Some(y) = year {
            params.push(("year", y.to_string()));
        }

        let response = self.client.get(&url).query(&params).send().await.ok()?;

        if !response.status().is_success() {
            tracing::error!("TMDB API error: {}", response.status());
            return None;
        }

        let search_response: TmdbSearchResponse = response.json().await.ok()?;

        if search_response.results.is_empty() {
            tracing::warn!("No TMDB results for movie: {}", title);
            return None;
        }

        let movie_id = search_response.results[0].id;

        // Get movie details to retrieve IMDb ID
        let _permit = self.rate_limiter.acquire().await;

        url = format!("{}/movie/{}", self.base_url, movie_id);
        let response = self
            .client
            .get(&url)
            .query(&[("api_key", &self.api_key)])
            .send()
            .await
            .ok()?;

        let details: TmdbMovieDetails = response.json().await.ok()?;

        let imdb_id = details.imdb_id.clone()?;
        let metadata =
            self.movie_metadata(imdb_id, &details, search_response.results[0].title.clone());

        tracing::info!("Found IMDb ID for {}: {}", title, metadata.imdb_id);

        // Cache the result
        self.cache.insert(cache_key, metadata.clone()).await;
        Some(metadata)
    }

    pub async fn search_tv_show(&self, title: &str, year: Option<u16>) -> Option<MediaMetadata> {
        let cache_key = format!("tv:{}:{}", title, year.unwrap_or(0));

        // Check cache first
        if let Some(cached) = self.cache.get(&cache_key).await {
            tracing::debug!("Cache hit for TV show: {}", title);
            return Some(cached);
        }

        // Rate limit
        let _permit = self.rate_limiter.acquire().await;

        // Search for TV show
        let mut url = format!("{}/search/tv", self.base_url);
        let mut params = vec![
            ("api_key", self.api_key.clone()),
            ("query", title.to_string()),
        ];
        if let Some(y) = year {
            params.push(("first_air_date_year", y.to_string()));
        }

        let response = self.client.get(&url).query(&params).send().await.ok()?;

        if !response.status().is_success() {
            tracing::error!("TMDB API error: {}", response.status());
            return None;
        }

        let search_response: TmdbSearchResponse = response.json().await.ok()?;

        if search_response.results.is_empty() {
            tracing::warn!("No TMDB results for TV show: {}", title);
            return None;
        }

        let tv_id = search_response.results[0].id;

        // Get external IDs to retrieve IMDb ID
        let _permit = self.rate_limiter.acquire().await;

        url = format!("{}/tv/{}/external_ids", self.base_url, tv_id);
        let response = self
            .client
            .get(&url)
            .query(&[("api_key", &self.api_key)])
            .send()
            .await
            .ok()?;

        let external_ids: TmdbTvExternalIds = response.json().await.ok()?;

        let imdb_id = external_ids.imdb_id?;

        // Get TV show details to retrieve poster
        let _permit = self.rate_limiter.acquire().await;

        let details_url = format!("{}/tv/{}", self.base_url, tv_id);
        let details_response = self
            .client
            .get(&details_url)
            .query(&[("api_key", &self.api_key)])
            .send()
            .await
            .ok()?;

        let details: TmdbTvDetails = details_response.json().await.ok()?;

        let metadata =
            self.tv_metadata(imdb_id, &details, search_response.results[0].title.clone());

        tracing::info!("Found IMDb ID for {}: {}", title, metadata.imdb_id);

        // Cache the result
        self.cache.insert(cache_key, metadata.clone()).await;
        Some(metadata)
    }

    pub async fn get_metadata_by_imdb_id(&self, imdb_id: &str) -> Option<MediaMetadata> {
        let cache_key = format!("imdb:{}", imdb_id);

        // Check cache first
        if let Some(cached) = self.cache.get(&cache_key).await {
            tracing::debug!("Cache hit for IMDb ID: {}", imdb_id);
            return Some(cached);
        }

        // Rate limit
        let _permit = self.rate_limiter.acquire().await;

        // Use TMDB find endpoint with IMDb ID
        let url = format!("{}/find/{}", self.base_url, imdb_id);
        let response = self
            .client
            .get(&url)
            .query(&[
                ("api_key", self.api_key.as_str()),
                ("external_source", "imdb_id"),
            ])
            .send()
            .await
            .ok()?;

        if !response.status().is_success() {
            tracing::error!("TMDB API error: {}", response.status());
            return None;
        }

        let find_response: TmdbFindResponse = response.json().await.ok()?;

        // Check movie results first, then TV results
        let metadata = if !find_response.movie_results.is_empty() {
            let result = &find_response.movie_results[0];
            MediaMetadata {
                imdb_id: imdb_id.to_string(),
                title: result.title.clone(),
                year: parse_year(&result.release_date),
                overview: result.overview.clone(),
                poster_url: result
                    .poster_path
                    .clone()
                    .map(|path| format!("{}{}", self.image_base_url, path)),
                tmdb_rating: result.vote_average,
                tmdb_votes: result.vote_count,
            }
        } else if !find_response.tv_results.is_empty() {
            let result = &find_response.tv_results[0];
            MediaMetadata {
                imdb_id: imdb_id.to_string(),
                title: result.title.clone(),
                year: parse_year(&result.first_air_date),
                overview: result.overview.clone(),
                poster_url: result
                    .poster_path
                    .clone()
                    .map(|path| format!("{}{}", self.image_base_url, path)),
                tmdb_rating: result.vote_average,
                tmdb_votes: result.vote_count,
            }
        } else {
            tracing::warn!("No TMDB results for IMDb ID: {}", imdb_id);
            return None;
        };

        tracing::info!("Found metadata for IMDb ID: {}", imdb_id);

        // Cache the result
        self.cache.insert(cache_key, metadata.clone()).await;
        Some(metadata)
    }

    fn movie_metadata(
        &self,
        imdb_id: String,
        details: &TmdbMovieDetails,
        fallback_title: Option<String>,
    ) -> MediaMetadata {
        MediaMetadata {
            imdb_id,
            title: details.title.clone().or(fallback_title),
            year: parse_year(&details.release_date),
            overview: details.overview.clone(),
            poster_url: details
                .poster_path
                .clone()
                .map(|path| format!("{}{}", self.image_base_url, path)),
            tmdb_rating: details.vote_average,
            tmdb_votes: details.vote_count,
        }
    }

    fn tv_metadata(
        &self,
        imdb_id: String,
        details: &TmdbTvDetails,
        fallback_title: Option<String>,
    ) -> MediaMetadata {
        MediaMetadata {
            imdb_id,
            title: details.name.clone().or(fallback_title),
            year: parse_year(&details.first_air_date),
            overview: details.overview.clone(),
            poster_url: details
                .poster_path
                .clone()
                .map(|path| format!("{}{}", self.image_base_url, path)),
            tmdb_rating: details.vote_average,
            tmdb_votes: details.vote_count,
        }
    }
}

fn parse_year(date: &Option<String>) -> Option<u16> {
    date.as_deref()?.get(..4)?.parse().ok()
}
