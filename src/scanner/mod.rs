pub mod parser;

use crate::config::Config;
use crate::index::types::{ContentType, FileInfo, ParsedMetadata};
use crate::index::MediaIndex;
use crate::metadata::TmdbClient;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parser::{dir_chain, extract_imdb_id, parse_filename, parse_season_episode, DirKind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use walkdir::WalkDir;

const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg", "m2ts", "ts", "vob",
];

pub struct MediaScanner {
    pub index: Arc<MediaIndex>,
    pub tmdb_client: Arc<TmdbClient>,
    pub config: Arc<Config>,
    pub scanning: Arc<AtomicBool>,
    pub pending_rescan: Arc<AtomicBool>,
}

impl MediaScanner {
    pub fn new(index: Arc<MediaIndex>, tmdb_client: Arc<TmdbClient>, config: Arc<Config>) -> Self {
        Self {
            index,
            tmdb_client,
            config,
            scanning: Arc::new(AtomicBool::new(false)),
            pending_rescan: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn start(&self) {
        tracing::info!("Starting media scanner");

        // Initial scan
        if let Err(e) = self.scan().await {
            tracing::error!("Initial scan failed: {}", e);
        }

        // Start file watching
        let scanner = self.clone_for_task();
        tokio::spawn(async move {
            if let Err(e) = scanner.watch_files().await {
                tracing::error!("File watcher failed: {}", e);
            }
        });

        // Start cron scheduler if SCAN_CRON is configured
        if let Some(ref cron_expr) = self.config.scan_cron {
            match self.config.parsed_scan_cron() {
                Ok(Some(schedule)) => {
                    tracing::info!("Scheduled media scanner with cron: {}", cron_expr);
                    let scanner = self.clone_for_task();
                    tokio::spawn(async move {
                        scanner.run_cron_scheduler(schedule).await;
                    });
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::error!("Invalid SCAN_CRON '{}': {}", cron_expr, e);
                }
            }
        }
    }

    async fn run_cron_scheduler(&self, schedule: cron::Schedule) {
        for next in schedule.upcoming(chrono::Local) {
            match (next - chrono::Local::now()).to_std() {
                Ok(duration) => {
                    tokio::time::sleep(duration).await;
                    tracing::info!("Running scheduled media library scan");
                    if let Err(e) = self.scan().await {
                        tracing::error!("Scheduled scan failed: {}", e);
                    }
                }
                Err(_) => {
                    tracing::warn!(
                        "Scheduled scan for {} was already due; skipping missed tick",
                        next
                    );
                }
            }
        }
    }

    fn clone_for_task(&self) -> Self {
        Self {
            index: Arc::clone(&self.index),
            tmdb_client: Arc::clone(&self.tmdb_client),
            config: Arc::clone(&self.config),
            scanning: Arc::clone(&self.scanning),
            pending_rescan: Arc::clone(&self.pending_rescan),
        }
    }

    async fn watch_files(&self) -> anyhow::Result<()> {
        tracing::info!("Starting file watcher for {:?}", self.config.media_path);

        let (tx, mut rx) = mpsc::channel(100);

        // Create watcher
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.blocking_send(event);
                }
            },
            notify::Config::default(),
        )?;

        // Watch the media directory recursively
        watcher.watch(&self.config.media_path, RecursiveMode::Recursive)?;

        tracing::info!("File watcher active");

        // Process events
        while let Some(event) = rx.recv().await {
            tracing::debug!("Watcher event: {:?}", event);
            match event.kind {
                EventKind::Create(_) => {
                    for path in event.paths {
                        self.handle_creation(&path).await;
                    }
                }
                EventKind::Remove(_) => {
                    for path in event.paths {
                        self.handle_removal(&path);
                    }
                }
                EventKind::Modify(_) => {
                    for path in event.paths {
                        self.handle_rename(&path).await;
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn is_existing_video_file(&self, path: &Path) -> bool {
        if !path.is_file() {
            return false;
        }

        self.is_video_file(path)
    }

    fn is_video_file(&self, path: &Path) -> bool {
        if let Some(ext) = path.extension() {
            if let Some(ext_str) = ext.to_str() {
                return VIDEO_EXTENSIONS.contains(&ext_str.to_lowercase().as_str());
            }
        }

        false
    }

    async fn handle_creation(&self, path: &Path) {
        if self.is_existing_video_file(path) {
            tracing::info!("Detected new file: {:?}", path);
            if let Err(e) = self.add_file(path).await {
                tracing::error!("Failed to add file {:?}: {}", path, e);
            }
        } else if path.is_dir() {
            tracing::info!("Detected new directory, scanning: {:?}", path);
            if let Err(e) = self.add_directory(path).await {
                tracing::error!("Failed to scan directory {:?}: {}", path, e);
            }
        }
    }

    fn handle_removal(&self, path: &Path) {
        if self.is_video_file(path) {
            tracing::info!("Detected removed file: {:?}", path);
            self.index.remove_by_path(path);
        } else if !path.exists() && path.extension().is_none() {
            tracing::info!("Detected removed directory, purging entries: {:?}", path);
            self.index.remove_by_dir(path);
        }
    }

    async fn handle_rename(&self, path: &Path) {
        if self.is_video_file(path) {
            if path.exists() {
                tracing::info!("Detected video file rename, found file: {:?}", path);
                if let Err(e) = self.add_file(path).await {
                    tracing::error!("Failed to add file {:?}: {}", path, e);
                }
            } else {
                tracing::info!(
                    "Detected video file rename, file gone, removing: {:?}",
                    path
                );
                self.index.remove_by_path(path);
            }
        } else if path.is_dir() {
            tracing::info!("Detected directory rename, rescanning: {:?}", path);
            if let Err(e) = self.add_directory(path).await {
                tracing::error!("Failed to scan directory {:?}: {}", path, e);
            }
        } else if !path.exists() && path.extension().is_none() {
            tracing::info!(
                "Detected directory rename, removing stale entries under: {:?}",
                path
            );
            self.index.remove_by_dir(path);
        }
    }

    async fn add_directory(&self, dir_path: &Path) -> anyhow::Result<()> {
        let files = self.scan_directory(dir_path)?;
        for path in files {
            if let Err(e) = self.index_file(&path).await {
                tracing::error!("Failed to index {:?}: {}", path, e);
            }
        }
        Ok(())
    }

    async fn add_file(&self, file_path: &Path) -> anyhow::Result<()> {
        let path = file_path
            .canonicalize()
            .unwrap_or_else(|_| file_path.to_path_buf());

        self.index_file(&path).await?;

        Ok(())
    }

    pub async fn scan(&self) -> anyhow::Result<()> {
        loop {
            if self
                .scanning
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                self.pending_rescan.store(true, Ordering::SeqCst);
                return Ok(());
            }

            let result = self.do_scan().await;
            self.scanning.store(false, Ordering::SeqCst);

            if self.pending_rescan.swap(false, Ordering::SeqCst) {
                tracing::info!("Queued rescan running after previous scan completed");
                continue;
            }

            return result;
        }
    }

    async fn do_scan(&self) -> anyhow::Result<()> {
        tracing::info!("Scanning media directory: {:?}", self.config.media_path);

        // Scan directory for video files
        let files = self.scan_directory(&self.config.media_path)?;
        tracing::info!("Found {} video files", files.len());

        // Clear and rebuild index
        self.index.clear();

        let mut successful = 0;
        let mut failed = 0;

        // Index each file
        for file_path in files {
            match self.index_file(&file_path).await {
                Ok(true) => successful += 1,
                Ok(false) => failed += 1,
                Err(e) => {
                    tracing::error!("Error indexing {:?}: {}", file_path, e);
                    failed += 1;
                }
            }
        }

        tracing::info!(
            "Scan complete: {} successful, {} failed",
            successful,
            failed
        );

        Ok(())
    }

    fn scan_directory(&self, dir_path: &Path) -> anyhow::Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for entry in WalkDir::new(dir_path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }

            if let Some(ext) = entry.path().extension() {
                if let Some(ext_str) = ext.to_str() {
                    if VIDEO_EXTENSIONS.contains(&ext_str.to_lowercase().as_str()) {
                        files.push(
                            entry
                                .path()
                                .canonicalize()
                                .unwrap_or_else(|_| entry.path().to_path_buf()),
                        );
                    }
                }
            }
        }

        Ok(files)
    }

    async fn index_file(&self, file_path: &Path) -> anyhow::Result<bool> {
        let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        tracing::debug!("Indexing: {}", file_name);

        // Parse filename
        let parsed = parse_filename(file_name);

        let mut title = parsed.title.clone();
        let mut year = parsed.year;

        // Check for IMDb ID override in filename
        let mut imdb_id = extract_imdb_id(file_name);

        // A season folder is a series signal on its own, so it counts even
        // without an SxxEyy in the filename.
        let dir_chain = dir_chain(file_path, &self.config.media_path);
        let season_from_dir = dir_chain.iter().find_map(|(_, kind)| match kind {
            DirKind::Season(season) => Some(*season),
            DirKind::Show | DirKind::Container => None,
        });
        let is_series = parsed.is_series || season_from_dir.is_some();

        // Only the season folder named the file an episode, so its filename
        // names the episode rather than the show.
        let named_after_episode = !parsed.is_series && season_from_dir.is_some();

        // For series, walk the folder chain above the file
        if is_series {
            // Any folder may hold an IMDb ID override, including one above the
            // season folder.
            if imdb_id.is_none() {
                imdb_id = dir_chain.iter().find_map(|(name, _)| extract_imdb_id(name));
            }

            // The nearest non-season, non-disc folder is the show, so
            // `Show (2008)/Season 1/ep.mkv` resolves exactly as a flat layout.
            let show_dir = dir_chain
                .iter()
                .find(|(_, kind)| *kind == DirKind::Show)
                .map(|(name, _)| *name);

            if let Some(show_dir) = show_dir {
                let show_parsed = parse_filename(show_dir);

                // Fall back to the show folder for the title. It also wins over
                // the filename when that names the episode.
                if (title.is_empty() || named_after_episode) && !show_parsed.title.is_empty() {
                    title = show_parsed.title.clone();
                }

                // The year usually lives only in the show folder name, and
                // without it a remake wins the TMDB search. Only the show folder
                // is read, so `TV (2019)/` cannot pass its year on.
                if year.is_none() {
                    year = show_parsed.year;
                }
            }
        }

        // After trying the folder chain for series, check if we have a title.
        // An IMDb ID override is enough on its own — TMDB supplies the title,
        // so an unparseable title must not drop the file.
        if title.is_empty() && imdb_id.is_none() {
            tracing::warn!(
                "Could not extract title from: {} {}",
                file_name,
                if is_series { "or the show folder" } else { "" }
            );
            return Ok(false);
        }

        // Lookup metadata via TMDB
        let metadata = if let Some(imdb_id) = imdb_id {
            tracing::debug!("Found IMDb ID override: {}", imdb_id);
            self.tmdb_client.get_metadata_by_imdb_id(&imdb_id).await
        } else if is_series {
            self.tmdb_client.search_tv_show(&title, year).await
        } else {
            self.tmdb_client.search_movie(&title, year).await
        };

        let Some(metadata) = metadata else {
            tracing::warn!("Could not find IMDb ID for: {}", title);
            return Ok(false);
        };

        if let Some(tmdb_title) = &metadata.title {
            title = tmdb_title.clone();
        }
        if metadata.year.is_some() {
            year = metadata.year;
        }

        // Create FileInfo
        let file_info = FileInfo {
            imdb_id: metadata.imdb_id.clone(),
            title,
            year,
            content_type: if is_series {
                ContentType::Series
            } else {
                ContentType::Movie
            },
            file_path: file_path.to_path_buf(),
            parsed: ParsedMetadata {
                // A season folder fills in a filename that omits it; on a
                // conflict the filename wins.
                season: parsed.season.or(season_from_dir),
                // Inside a season folder a bare number is the episode, not
                // part of the title: "Season 01/01 - Pilot.mkv" or
                // "S01/Pilot.01.mkv".
                episode: parsed
                    .episode
                    .or_else(|| season_from_dir.and_then(|_| parse_season_episode(file_name))),
            },
            poster: self
                .config
                .poster_url_for(&metadata.imdb_id)
                .or_else(|| metadata.poster_url.clone()),
            description: metadata.overview.clone(),
            tmdb_rating: metadata.tmdb_rating,
            tmdb_votes: metadata.tmdb_votes,
        };

        // Add to index
        match file_info.content_type {
            ContentType::Movie => {
                self.index.insert_movie(metadata.imdb_id.clone(), file_info);
            }
            ContentType::Series => {
                self.index.insert_episode(metadata.imdb_id, file_info);
            }
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::index::types::IndexEntry;
    use crate::index::MediaIndex;
    use crate::metadata::TmdbClient;
    use std::sync::atomic::Ordering;

    fn make_scanner() -> MediaScanner {
        make_scanner_with_tmdb("http://localhost")
    }

    fn make_scanner_with_tmdb(tmdb_base_url: &str) -> MediaScanner {
        make_scanner_for_media(
            tmdb_base_url,
            std::path::Path::new("/tmp/lanio_test_nonexistent"),
        )
    }

    fn make_scanner_for_media(tmdb_base_url: &str, media_path: &Path) -> MediaScanner {
        let config = Arc::new(Config {
            media_path: media_path.to_path_buf(),
            port: 8078,
            base_url: None,
            public_url: None,
            tmdb_api_key: "fake".to_string(),
            tmdb_base_url: tmdb_base_url.to_string(),
            tmdb_image_base_url: "http://localhost".to_string(),
            password: None,
            auth_token: None,
            poster_url: None,
            scan_cron: None,
        });
        MediaScanner::new(
            Arc::new(MediaIndex::new()),
            Arc::new(TmdbClient::new(
                "fake".to_string(),
                tmdb_base_url.to_string(),
                "http://localhost".to_string(),
            )),
            config,
        )
    }

    /// Mocks a TV search that only answers for `query` at `year`, so a lookup
    /// that omits or mistypes the year is left unmatched.
    fn mock_series(server: &httpmock::MockServer, query: &str, year: &str, id: u32, imdb: &str) {
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/search/tv")
                .query_param("query", query)
                .query_param("first_air_date_year", year);
            then.status(200).json_body(serde_json::json!({
                "results": [
                    { "id": 9999, "name": "Some Other Show", "first_air_date": "2021-01-01" },
                    { "id": id, "name": query, "first_air_date": format!("{year}-01-20") }
                ]
            }));
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path(format!("/tv/{}/external_ids", id));
            then.status(200)
                .json_body(serde_json::json!({ "imdb_id": imdb }));
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path(format!("/tv/{}", id));
            then.status(200).json_body(serde_json::json!({
                "name": query,
                "first_air_date": format!("{year}-01-20")
            }));
        });
    }

    #[tokio::test]
    async fn scanning_flag_false_after_scan_completes() {
        let scanner = make_scanner();
        assert!(!scanner.scanning.load(Ordering::SeqCst));
        // scan() should always reset the flag, even if do_scan returns an error
        let _ = scanner.scan().await;
        assert!(
            !scanner.scanning.load(Ordering::SeqCst),
            "scanning flag must be false after scan() returns"
        );
    }

    #[tokio::test]
    async fn concurrent_scan_skipped_while_in_progress() {
        let scanner = make_scanner();
        // Simulate a scan already in progress
        scanner.scanning.store(true, Ordering::SeqCst);
        // A second call should return Ok immediately and queue a follow-up scan
        let result = scanner.scan().await;
        assert!(result.is_ok());
        assert!(
            scanner.scanning.load(Ordering::SeqCst),
            "flag should remain true — only the original caller should reset it"
        );
        assert!(
            scanner.pending_rescan.load(Ordering::SeqCst),
            "request while scanning should be queued"
        );
        // Clean up
        scanner.scanning.store(false, Ordering::SeqCst);
        scanner.pending_rescan.store(false, Ordering::SeqCst);
    }

    #[tokio::test]
    async fn queued_rescan_consumed_and_scan_completes() {
        let scanner = make_scanner();
        // Simulate a queued rescan from a request that arrived mid-scan
        scanner.pending_rescan.store(true, Ordering::SeqCst);

        let result = scanner.scan().await;
        assert!(result.is_ok());
        assert!(
            !scanner.pending_rescan.load(Ordering::SeqCst),
            "pending rescan should be consumed after the scan completes"
        );
        assert!(!scanner.scanning.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn scanner_starts_with_scan_cron_configured() {
        let config = Config {
            media_path: std::path::PathBuf::from("/tmp/lanio_test_nonexistent"),
            port: 8078,
            base_url: None,
            public_url: None,
            tmdb_api_key: "fake".to_string(),
            tmdb_base_url: "http://localhost".to_string(),
            tmdb_image_base_url: "http://localhost".to_string(),
            password: None,
            auth_token: None,
            poster_url: None,
            scan_cron: Some("0 3 * * *".to_string()),
        };
        let scanner = MediaScanner::new(
            Arc::new(MediaIndex::new()),
            Arc::new(TmdbClient::new(
                "fake".to_string(),
                "http://localhost".to_string(),
                "http://localhost".to_string(),
            )),
            Arc::new(config),
        );
        scanner.start().await;
        assert!(!scanner.scanning.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn indexes_numeric_titled_movie_with_imdb_id_override() {
        let server = httpmock::MockServer::start();
        server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/find/tt1099212");
            then.status(200).json_body(serde_json::json!({
                "movie_results": [{
                    "title": "2012",
                    "overview": "A Mayan apocalypse.",
                    "release_date": "2009-11-13",
                    "poster_path": "/poster.jpg",
                    "vote_average": 6.8,
                    "vote_count": 9000
                }],
                "tv_results": []
            }));
        });

        let scanner = make_scanner_with_tmdb(&server.base_url());
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("2012.tt1099212.mkv");
        std::fs::write(&file, b"").unwrap();

        assert!(
            scanner.index_file(&file).await.unwrap(),
            "a file with a valid IMDb ID must be indexed even when the \
             parsed title is empty"
        );
        assert!(matches!(
            scanner.index.get("tt1099212"),
            Some(IndexEntry::Movie(_))
        ));
    }

    #[tokio::test]
    async fn indexes_numeric_titled_movie_by_title_search() {
        let server = httpmock::MockServer::start();
        let details = serde_json::json!({
            "id": 65754,
            "imdb_id": "tt1099212",
            "title": "2012",
            "overview": "A Mayan apocalypse.",
            "release_date": "2009-11-13",
            "poster_path": "/poster.jpg",
            "vote_average": 6.8,
            "vote_count": 9000
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/search/movie")
                .query_param("query", "2012")
                .query_param("year", "2009");
            then.status(200)
                .json_body(serde_json::json!({ "results": [details] }));
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/movie/65754");
            then.status(200).json_body(details);
        });

        let scanner = make_scanner_with_tmdb(&server.base_url());
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("2012.2009.1080p.BluRay.mkv");
        std::fs::write(&file, b"").unwrap();

        assert!(scanner.index_file(&file).await.unwrap());

        let Some(IndexEntry::Movie(indexed)) = scanner.index.get("tt1099212") else {
            panic!("expected 2012 to be indexed as a movie");
        };
        assert_eq!(indexed.title, "2012");
        assert_eq!(indexed.year, Some(2009));
    }

    /// `Avatar The Last Airbender (2005)/Avatar.The.Last.Airbender.S01E01.mkv`
    /// — the title lives in the episode filename, so the `(2005)` in the
    /// folder name must still be used to disambiguate the TMDB lookup.
    #[tokio::test]
    async fn series_year_from_parent_directory_disambiguates_tmdb_search() {
        let server = httpmock::MockServer::start();
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/search/tv")
                .query_param("query", "Avatar The Last Airbender")
                .query_param("first_air_date_year", "2005");
            then.status(200).json_body(serde_json::json!({
                "results": [
                    { "id": 100, "name": "Avatar: The Last Airbender", "first_air_date": "2024-02-07" },
                    { "id": 200, "name": "Avatar: The Last Airbender", "first_air_date": "2005-02-08" }
                ]
            }));
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/tv/200/external_ids");
            then.status(200)
                .json_body(serde_json::json!({ "imdb_id": "tt0417299" }));
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/tv/200");
            then.status(200).json_body(serde_json::json!({
                "name": "Avatar: The Last Airbender",
                "first_air_date": "2005-02-08"
            }));
        });

        let scanner = make_scanner_with_tmdb(&server.base_url());
        let dir = tempfile::tempdir().unwrap();
        let show = dir.path().join("Avatar The Last Airbender (2005)");
        std::fs::create_dir_all(&show).unwrap();
        let file = show.join("Avatar.The.Last.Airbender.S01E01.mkv");
        std::fs::write(&file, b"").unwrap();

        assert!(scanner.index_file(&file).await.unwrap());
        assert!(
            matches!(scanner.index.get("tt0417299"), Some(IndexEntry::Series(_))),
            "the 2005 series must be indexed using the year from the parent directory"
        );
    }

    /// The season folder sits between the file and the show folder, so the
    /// `(2008)` must still reach the TMDB query.
    #[tokio::test]
    async fn series_year_read_from_show_folder_through_season_folder() {
        let server = httpmock::MockServer::start();
        mock_series(&server, "Breaking Bad", "2008", 1396, "tt0903747");

        let dir = tempfile::tempdir().unwrap();
        let scanner = make_scanner_for_media(&server.base_url(), dir.path());
        let file = dir
            .path()
            .join("Breaking Bad (2008)")
            .join("Season 1")
            .join("Breaking.Bad.S01E01.mkv");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"").unwrap();

        assert!(scanner.index_file(&file).await.unwrap());
        let Some(IndexEntry::Series(episodes)) = scanner.index.get("tt0903747") else {
            panic!("expected the season folder not to hide the 2008 series");
        };
        assert_eq!(episodes[0].parsed.season, Some(1));
        assert_eq!(episodes[0].parsed.episode, Some(1));
    }

    /// The IMDb ID lives in the show folder, above the season folder.
    #[tokio::test]
    async fn imdb_id_read_from_show_folder_through_season_folder() {
        let server = httpmock::MockServer::start();
        server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/find/tt0903747");
            then.status(200).json_body(serde_json::json!({
                "movie_results": [],
                "tv_results": [{
                    "name": "Breaking Bad",
                    "first_air_date": "2008-01-20",
                    "poster_path": "/poster.jpg"
                }]
            }));
        });

        let dir = tempfile::tempdir().unwrap();
        let scanner = make_scanner_for_media(&server.base_url(), dir.path());
        let file = dir
            .path()
            .join("Breaking Bad (2008) tt0903747")
            .join("Season 1")
            .join("Breaking.Bad.S01E01.mkv");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"").unwrap();

        assert!(scanner.index_file(&file).await.unwrap());
        assert!(matches!(
            scanner.index.get("tt0903747"),
            Some(IndexEntry::Series(_))
        ));
    }

    /// No SxxEyy: the season folder alone makes this an episode, and the show
    /// folder supplies the title because the filename names the episode.
    #[tokio::test]
    async fn episode_without_se_number_in_season_folder_is_indexed_as_series() {
        let server = httpmock::MockServer::start();
        mock_series(&server, "Breaking Bad", "2008", 1396, "tt0903747");

        let dir = tempfile::tempdir().unwrap();
        let scanner = make_scanner_for_media(&server.base_url(), dir.path());
        let file = dir
            .path()
            .join("Breaking Bad (2008)")
            .join("Season 1")
            .join("Pilot.mkv");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"").unwrap();

        assert!(scanner.index_file(&file).await.unwrap());
        let Some(IndexEntry::Series(episodes)) = scanner.index.get("tt0903747") else {
            panic!("a file in a season folder must not be indexed as a movie");
        };
        assert_eq!(episodes[0].parsed.season, Some(1));
        assert_eq!(
            episodes[0].parsed.episode, None,
            "no episode number is available without an SxxEyy filename"
        );
    }

    /// `Specials` is season 0, matching the S00Exx filename form.
    #[tokio::test]
    async fn specials_folder_maps_to_season_zero() {
        let server = httpmock::MockServer::start();
        mock_series(&server, "Breaking Bad", "2008", 1396, "tt0903747");

        let dir = tempfile::tempdir().unwrap();
        let scanner = make_scanner_for_media(&server.base_url(), dir.path());
        let file = dir
            .path()
            .join("Breaking Bad (2008)")
            .join("Specials")
            .join("Better Call Saul - El Camino.mkv");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"").unwrap();

        assert!(scanner.index_file(&file).await.unwrap());
        let Some(IndexEntry::Series(episodes)) = scanner.index.get("tt0903747") else {
            panic!("a file in a Specials folder must be indexed as a series");
        };
        assert_eq!(episodes[0].parsed.season, Some(0));
    }

    /// `Season 01/01 - Pilot.mkv` has no SxxEyy, so the season folder gives the
    /// season and the filename's leading number gives the episode.
    #[tokio::test]
    async fn leading_episode_number_inside_season_folder_is_the_episode() {
        let server = httpmock::MockServer::start();
        mock_series(&server, "Breaking Bad", "2008", 1396, "tt0903747");

        let dir = tempfile::tempdir().unwrap();
        let scanner = make_scanner_for_media(&server.base_url(), dir.path());
        let file = dir
            .path()
            .join("Breaking Bad (2008)")
            .join("Season 01")
            .join("01 - Pilot.mkv");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"").unwrap();

        assert!(scanner.index_file(&file).await.unwrap());
        let Some(IndexEntry::Series(episodes)) = scanner.index.get("tt0903747") else {
            panic!("expected the episode to be indexed against the show folder");
        };
        assert_eq!(episodes[0].parsed.season, Some(1));
        assert_eq!(episodes[0].parsed.episode, Some(1));
        assert_eq!(episodes[0].title, "Breaking Bad");
    }

    /// `S01/Pilot.01.mkv` — the bare season folder form, with the episode
    /// number trailing the title.
    #[tokio::test]
    async fn trailing_episode_number_in_bare_season_folder_is_the_episode() {
        let server = httpmock::MockServer::start();
        mock_series(&server, "Breaking Bad", "2008", 1396, "tt0903747");

        let dir = tempfile::tempdir().unwrap();
        let scanner = make_scanner_for_media(&server.base_url(), dir.path());
        let file = dir
            .path()
            .join("Breaking Bad (2008)")
            .join("S01")
            .join("Pilot.01.mkv");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"").unwrap();

        assert!(scanner.index_file(&file).await.unwrap());
        let Some(IndexEntry::Series(episodes)) = scanner.index.get("tt0903747") else {
            panic!("expected the episode to be indexed against the show folder");
        };
        assert_eq!(episodes[0].parsed.season, Some(1));
        assert_eq!(episodes[0].parsed.episode, Some(1));
        assert_eq!(episodes[0].title, "Breaking Bad");
    }

    /// `Pilot.01.1080p.mkv` — the episode number trails the title, and the
    /// release tags behind it must not hide the number.
    #[tokio::test]
    async fn episode_number_is_read_from_before_the_release_tags() {
        let server = httpmock::MockServer::start();
        mock_series(&server, "Breaking Bad", "2008", 1396, "tt0903747");

        let dir = tempfile::tempdir().unwrap();
        let scanner = make_scanner_for_media(&server.base_url(), dir.path());
        let file = dir
            .path()
            .join("Breaking Bad (2008)")
            .join("Season 01")
            .join("Pilot.01.1080p.mkv");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"").unwrap();

        assert!(scanner.index_file(&file).await.unwrap());
        let Some(IndexEntry::Series(episodes)) = scanner.index.get("tt0903747") else {
            panic!("expected the episode to be indexed against the show folder");
        };
        assert_eq!(episodes[0].parsed.season, Some(1));
        assert_eq!(episodes[0].parsed.episode, Some(1));
    }

    /// `Pilot (01) 1080p.mkv` — a bracketed number is still the episode, and
    /// the year-like digits in `1080p` are not.
    #[tokio::test]
    async fn bracketed_episode_number_inside_season_folder_is_the_episode() {
        let server = httpmock::MockServer::start();
        mock_series(&server, "Breaking Bad", "2008", 1396, "tt0903747");

        let dir = tempfile::tempdir().unwrap();
        let scanner = make_scanner_for_media(&server.base_url(), dir.path());
        let file = dir
            .path()
            .join("Breaking Bad (2008)")
            .join("Season 01")
            .join("Pilot (01) 1080p.mkv");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"").unwrap();

        assert!(scanner.index_file(&file).await.unwrap());
        let Some(IndexEntry::Series(episodes)) = scanner.index.get("tt0903747") else {
            panic!("expected the episode to be indexed against the show folder");
        };
        assert_eq!(episodes[0].parsed.season, Some(1));
        assert_eq!(episodes[0].parsed.episode, Some(1));
    }

    /// A resolution tag must not be read as an episode number.
    #[tokio::test]
    async fn quality_tag_is_not_mistaken_for_an_episode_number() {
        let server = httpmock::MockServer::start();
        mock_series(&server, "Breaking Bad", "2008", 1396, "tt0903747");

        let dir = tempfile::tempdir().unwrap();
        let scanner = make_scanner_for_media(&server.base_url(), dir.path());
        let file = dir
            .path()
            .join("Breaking Bad (2008)")
            .join("Season 01")
            .join("Pilot.1080p.WEB-DL.mkv");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"").unwrap();

        assert!(scanner.index_file(&file).await.unwrap());
        let Some(IndexEntry::Series(episodes)) = scanner.index.get("tt0903747") else {
            panic!("expected the episode to be indexed against the show folder");
        };
        assert_eq!(episodes[0].parsed.season, Some(1));
        assert_eq!(
            episodes[0].parsed.episode, None,
            "1080p must not be read as episode 1080"
        );
    }

    /// An SxxEyy filename is more specific than its folder, so it keeps its
    /// own season.
    #[tokio::test]
    async fn filename_season_wins_over_season_folder() {
        let server = httpmock::MockServer::start();
        mock_series(&server, "Breaking Bad", "2008", 1396, "tt0903747");

        let dir = tempfile::tempdir().unwrap();
        let scanner = make_scanner_for_media(&server.base_url(), dir.path());
        let file = dir
            .path()
            .join("Breaking Bad (2008)")
            .join("Season 1")
            .join("Breaking.Bad.S02E05.mkv");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"").unwrap();

        assert!(scanner.index_file(&file).await.unwrap());
        let Some(IndexEntry::Series(episodes)) = scanner.index.get("tt0903747") else {
            panic!("expected Breaking Bad to be indexed");
        };
        assert_eq!(episodes[0].parsed.season, Some(2));
        assert_eq!(episodes[0].parsed.episode, Some(5));
    }

    /// A disc folder sits between the file and the show folder, and must not
    /// be read as the show.
    #[tokio::test]
    async fn disc_folder_inside_season_does_not_hide_the_show_folder() {
        let server = httpmock::MockServer::start();
        mock_series(&server, "Breaking Bad", "2008", 1396, "tt0903747");

        let dir = tempfile::tempdir().unwrap();
        let scanner = make_scanner_for_media(&server.base_url(), dir.path());
        let file = dir
            .path()
            .join("Breaking Bad (2008)")
            .join("Season 1")
            .join("Disc 1")
            .join("Breaking.Bad.S01E01.mkv");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"").unwrap();

        assert!(scanner.index_file(&file).await.unwrap());
        let Some(IndexEntry::Series(episodes)) = scanner.index.get("tt0903747") else {
            panic!("the show folder must be read through the disc folder");
        };
        assert_eq!(episodes[0].parsed.season, Some(1));
        assert_eq!(episodes[0].parsed.episode, Some(1));
    }

    /// The show folder is above the season folder here, so nothing claims the
    /// file as an episode.
    #[tokio::test]
    async fn movie_beneath_a_season_folder_is_not_an_episode() {
        let server = httpmock::MockServer::start();
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/search/movie")
                .query_param("query", "Sintel")
                .query_param("year", "2010");
            then.status(200).json_body(serde_json::json!({
                "results": [{
                    "id": 45745, "title": "Sintel", "release_date": "2010-09-27"
                }]
            }));
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/movie/45745");
            then.status(200).json_body(serde_json::json!({
                "imdb_id": "tt1727587",
                "title": "Sintel",
                "release_date": "2010-09-27"
            }));
        });

        let dir = tempfile::tempdir().unwrap();
        let scanner = make_scanner_for_media(&server.base_url(), dir.path());
        let file = dir
            .path()
            .join("Season 1")
            .join("Sintel (2010)")
            .join("Sintel.2010.1080p.mkv");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"").unwrap();

        assert!(scanner.index_file(&file).await.unwrap());
        assert!(
            matches!(scanner.index.get("tt1727587"), Some(IndexEntry::Movie(_))),
            "only a season folder that owns the file makes it an episode"
        );
    }

    /// The mock answers only a yearless query, so borrowing 2019 from
    /// `TV (2019)` would leave the file unindexed.
    #[tokio::test]
    async fn year_comes_from_show_folder_not_a_parent_category() {
        let server = httpmock::MockServer::start();
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/search/tv")
                .query_param("query", "Breaking Bad");
            then.status(200).json_body(serde_json::json!({
                "results": [
                    { "id": 1396, "name": "Breaking Bad", "first_air_date": "2008-01-20" }
                ]
            }));
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/tv/1396/external_ids");
            then.status(200)
                .json_body(serde_json::json!({ "imdb_id": "tt0903747" }));
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/tv/1396");
            then.status(200).json_body(serde_json::json!({
                "name": "Breaking Bad",
                "first_air_date": "2008-01-20"
            }));
        });

        let dir = tempfile::tempdir().unwrap();
        let scanner = make_scanner_for_media(&server.base_url(), dir.path());
        let file = dir
            .path()
            .join("TV (2019)")
            .join("Breaking Bad")
            .join("Season 1")
            .join("Breaking.Bad.S01E01.mkv");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"").unwrap();

        assert!(
            scanner.index_file(&file).await.unwrap(),
            "the show folder carries no year, so the lookup must proceed \
             without one rather than borrowing 2019 from TV (2019)"
        );
        assert!(matches!(
            scanner.index.get("tt0903747"),
            Some(IndexEntry::Series(_))
        ));
    }
}
