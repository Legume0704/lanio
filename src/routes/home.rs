use crate::config::Config;
use askama::Template;
use axum::{extract::State, response::Html};
use std::sync::Arc;

#[derive(Template)]
#[template(path = "home_auth.html")]
struct AuthPage {
    server_base: String,
}

#[derive(Template)]
#[template(path = "home_main.html")]
struct MainPage {
    manifest_url: String,
    install_url: String,
}

pub async fn home_handler(State(config): State<Arc<Config>>) -> Html<String> {
    if config.auth_token.is_some() {
        let server_base = config
            .public_url
            .as_ref()
            .or(config.base_url.as_ref())
            .map(|s| s.trim_end_matches('/').to_string())
            .unwrap_or_default();

        Html(AuthPage { server_base }.render().unwrap())
    } else {
        let manifest_url = config
            .public_url
            .as_ref()
            .or(config.base_url.as_ref())
            .map(|s| format!("{}/manifest.json", s.trim_end_matches('/')))
            .unwrap_or_else(|| format!("http://localhost:{}/manifest.json", config.port));

        let install_url = format!(
            "https://web.stremio.com/#/addons?addon={}",
            urlencoding::encode(&manifest_url)
        );

        Html(MainPage { manifest_url, install_url }.render().unwrap())
    }
}
