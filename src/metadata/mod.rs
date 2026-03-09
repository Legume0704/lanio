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
    pub fn new(api_key: String) -> Self {
        let cache = Cache::builder()
            .max_capacity(1000)
            .time_to_live(Duration::from_secs(86400)) // 24 hours
            .build();

        Self {
            client: reqwest::Client::new(),
            api_key,
            cache,
            rate_limiter: RateLimiter::new(40), // 40 requests per 10 seconds
            base_url: "https://api.themoviedb.org/3".to_string(),
            image_base_url: "https://image.tmdb.org/t/p/w500".to_string(),
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
        self.rate_limiter.acquire().await;

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
        self.rate_limiter.acquire().await;

        url = format!("{}/movie/{}", self.base_url, movie_id);
        let response = self
            .client
            .get(&url)
            .query(&[("api_key", &self.api_key)])
            .send()
            .await
            .ok()?;

        let details: TmdbMovieDetails = response.json().await.ok()?;

        let imdb_id = details.imdb_id?;
        let poster_url = details
            .poster_path
            .map(|path| format!("{}{}", self.image_base_url, path));

        tracing::info!("Found IMDb ID for {}: {}", title, imdb_id);

        let metadata = MediaMetadata {
            imdb_id,
            poster_url,
        };

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
        self.rate_limiter.acquire().await;

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
        self.rate_limiter.acquire().await;

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
        self.rate_limiter.acquire().await;

        let details_url = format!("{}/tv/{}", self.base_url, tv_id);
        let details_response = self
            .client
            .get(&details_url)
            .query(&[("api_key", &self.api_key)])
            .send()
            .await
            .ok()?;

        let details: TmdbTvDetails = details_response.json().await.ok()?;

        let poster_url = details
            .poster_path
            .map(|path| format!("{}{}", self.image_base_url, path));

        tracing::info!("Found IMDb ID for {}: {}", title, imdb_id);

        let metadata = MediaMetadata {
            imdb_id,
            poster_url,
        };

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
        self.rate_limiter.acquire().await;

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
        let poster_path = if !find_response.movie_results.is_empty() {
            find_response.movie_results[0].poster_path.clone()
        } else if !find_response.tv_results.is_empty() {
            find_response.tv_results[0].poster_path.clone()
        } else {
            tracing::warn!("No TMDB results for IMDb ID: {}", imdb_id);
            None
        };

        let poster_url = poster_path.map(|path| format!("{}{}", self.image_base_url, path));

        let metadata = MediaMetadata {
            imdb_id: imdb_id.to_string(),
            poster_url,
        };

        // Cache the result
        self.cache.insert(cache_key, metadata.clone()).await;
        Some(metadata)
    }
}
