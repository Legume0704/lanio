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

    #[serde(default = "default_tmdb_base_url")]
    pub tmdb_base_url: String,

    #[serde(default = "default_tmdb_image_base_url")]
    pub tmdb_image_base_url: String,

    /// Optional password for protecting Stremio routes.
    /// When set, all Stremio routes are prefixed with a 256-character token derived from this password.
    #[serde(default)]
    pub password: Option<String>,

    /// Pre-computed auth token derived from PASSWORD. Not read from env — set in from_env().
    #[serde(skip)]
    pub auth_token: Option<String>,

    /// Optional custom poster URL template. Supports `{imdb_id}` placeholder.
    /// Example: https://btttr.cc/poster/imdb/poster-default/{imdb_id}.jpg
    #[serde(default)]
    pub poster_url: Option<String>,
}

fn default_media_path() -> PathBuf {
    PathBuf::from("/media")
}

fn default_port() -> u16 {
    8078
}

fn default_tmdb_base_url() -> String {
    "https://api.themoviedb.org/3".to_string()
}

fn default_tmdb_image_base_url() -> String {
    "https://image.tmdb.org/t/p/w500".to_string()
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

    /// Returns a poster URL for the given IMDb ID if `poster_url` is configured.
    /// Replaces `{imdb_id}`, `{imdbId}`, or `{id}` placeholders with the provided IMDb ID.
    pub fn poster_url_for(&self, imdb_id: &str) -> Option<String> {
        self.poster_url.as_ref().and_then(|template| {
            let trimmed = template.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(
                    trimmed
                        .replace("{imdb_id}", imdb_id)
                        .replace("{imdbId}", imdb_id)
                        .replace("{id}", imdb_id),
                )
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_password(password: &str) -> Config {
        Config {
            media_path: default_media_path(),
            port: default_port(),
            base_url: None,
            public_url: None,
            tmdb_api_key: String::new(),
            tmdb_base_url: default_tmdb_base_url(),
            tmdb_image_base_url: default_tmdb_image_base_url(),
            password: Some(password.to_string()),
            auth_token: Some(compute_token(password)),
            poster_url: None,
        }
    }

    fn config_no_auth() -> Config {
        Config {
            media_path: default_media_path(),
            port: default_port(),
            base_url: None,
            public_url: None,
            tmdb_api_key: String::new(),
            tmdb_base_url: default_tmdb_base_url(),
            tmdb_image_base_url: default_tmdb_image_base_url(),
            password: None,
            auth_token: None,
            poster_url: None,
        }
    }

    #[test]
    fn valid_token_accepted() {
        let config = config_with_password("secret");
        let token = compute_token("secret");
        assert!(config.is_valid_token(&token));
    }

    #[test]
    fn wrong_token_rejected() {
        let config = config_with_password("secret");
        let wrong = compute_token("wrong_password");
        assert!(!config.is_valid_token(&wrong));
    }

    #[test]
    fn no_auth_accepts_any_token() {
        let config = config_no_auth();
        assert!(config.is_valid_token("anything"));
        assert!(config.is_valid_token(""));
    }

    #[test]
    fn empty_string_token_rejected_when_auth_set() {
        let config = config_with_password("secret");
        assert!(!config.is_valid_token(""));
    }

    #[test]
    fn auth_token_derived_from_password() {
        let config = config_with_password("mypass");
        assert_eq!(
            config.auth_token.as_deref(),
            Some(compute_token("mypass").as_str())
        );
    }

    #[test]
    fn poster_url_for_none_when_unconfigured() {
        let config = config_no_auth();
        assert_eq!(config.poster_url_for("tt1234567"), None);
    }

    #[test]
    fn poster_url_for_none_when_empty_or_whitespace() {
        let mut config = config_no_auth();
        config.poster_url = Some("".to_string());
        assert_eq!(config.poster_url_for("tt1234567"), None);

        config.poster_url = Some("   ".to_string());
        assert_eq!(config.poster_url_for("tt1234567"), None);
    }

    #[test]
    fn poster_url_for_replaces_imdb_id_placeholder() {
        let mut config = config_no_auth();
        config.poster_url =
            Some("https://btttr.cc/poster/imdb/poster-default/{imdb_id}.jpg".to_string());
        assert_eq!(
            config.poster_url_for("tt1234567"),
            Some("https://btttr.cc/poster/imdb/poster-default/tt1234567.jpg".to_string())
        );
    }

    #[test]
    fn poster_url_for_supports_alternative_placeholders() {
        let mut config = config_no_auth();
        config.poster_url = Some("https://example.com/posters/{imdbId}.png".to_string());
        assert_eq!(
            config.poster_url_for("tt1234567"),
            Some("https://example.com/posters/tt1234567.png".to_string())
        );

        config.poster_url = Some("https://example.com/posters/{id}.png".to_string());
        assert_eq!(
            config.poster_url_for("tt1234567"),
            Some("https://example.com/posters/tt1234567.png".to_string())
        );
    }

    #[test]
    fn config_from_env_parses_poster_url() {
        std::env::set_var("TMDB_API_KEY", "test_key");
        std::env::set_var(
            "POSTER_URL",
            "https://btttr.cc/poster/imdb/poster-default/{imdb_id}.jpg",
        );
        let config = Config::from_env().unwrap();
        assert_eq!(
            config.poster_url.as_deref(),
            Some("https://btttr.cc/poster/imdb/poster-default/{imdb_id}.jpg")
        );
        std::env::remove_var("POSTER_URL");
    }
}
