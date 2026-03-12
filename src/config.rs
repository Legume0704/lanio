use crate::auth::compute_token;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_media_path")]
    pub media_path: PathBuf,

    #[serde(default = "default_port")]
    pub port: u16,

    /// Base URL for video streaming (used in stream URLs returned to Stremio).
    /// Defaults to http://localhost:{port} if not set.
    #[serde(default)]
    pub base_url: Option<String>,

    /// Public URL for advertising the manifest install URL.
    /// If set, this is used in the home page and startup logs instead of base_url.
    /// Useful when the server is behind a reverse proxy or accessible publicly
    /// but streams video over a different (e.g. local) URL.
    #[serde(default)]
    pub public_url: Option<String>,

    pub tmdb_api_key: String,

    /// Optional password for protecting Stremio routes.
    /// When set, all Stremio routes are prefixed with a 256-character token derived from this password.
    #[serde(default)]
    pub password: Option<String>,

    /// Pre-computed auth token derived from PASSWORD. Not read from env — set in from_env().
    #[serde(skip)]
    pub auth_token: Option<String>,
}

fn default_media_path() -> PathBuf {
    PathBuf::from("/media")
}

fn default_port() -> u16 {
    8078
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let mut config: Config = envy::from_env()
            .map_err(|e| anyhow::anyhow!("Failed to load config from environment: {}", e))?;
        config.auth_token = config.password.as_ref().map(|p| compute_token(p));
        Ok(config)
    }

    /// Returns true if the given token matches the configured auth token,
    /// or if no password is configured.
    pub fn is_valid_token(&self, token: &str) -> bool {
        match &self.auth_token {
            Some(expected) => token == expected.as_str(),
            None => true,
        }
    }
}
