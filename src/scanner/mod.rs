pub mod parser;

use crate::config::Config;
use crate::index::types::{ContentType, FileInfo, ParsedMetadata};
use crate::index::MediaIndex;
use crate::metadata::TmdbClient;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parser::{extract_imdb_id, parse_filename};
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

        // For series, check parent directory
        if parsed.is_series {
            if let Some(parent) = file_path.parent() {
                if let Some(parent_name) = parent.file_name().and_then(|n| n.to_str()) {
                    // Check for IMDb ID in parent directory
                    if imdb_id.is_none() {
                        imdb_id = extract_imdb_id(parent_name);
                    }

                    // Use parent directory as series title if filename didn't have one
                    if title.is_empty() {
                        let parent_parsed = parse_filename(parent_name);
                        if !parent_parsed.title.is_empty() {
                            title = parent_parsed.title;
                            if year.is_none() {
                                year = parent_parsed.year;
                            }
                        }
                    }
                }
            }
        }

        // After trying parent directory for series, check if we have a title
        if title.is_empty() {
            tracing::warn!(
                "Could not extract title from: {} {}",
                file_name,
                if parsed.is_series {
                    "or parent directory"
                } else {
                    ""
                }
            );
            return Ok(false);
        }

        // Lookup metadata via TMDB
        let metadata = if let Some(imdb_id) = imdb_id {
            tracing::debug!("Found IMDb ID override: {}", imdb_id);
            self.tmdb_client.get_metadata_by_imdb_id(&imdb_id).await
        } else if parsed.is_series {
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
            content_type: if parsed.is_series {
                ContentType::Series
            } else {
                ContentType::Movie
            },
            file_path: file_path.to_path_buf(),
            parsed: ParsedMetadata {
                season: parsed.season,
                episode: parsed.episode,
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
    use crate::index::MediaIndex;
    use crate::metadata::TmdbClient;
    use std::sync::atomic::Ordering;

    fn make_scanner() -> MediaScanner {
        let config = Arc::new(Config {
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
            scan_cron: None,
        });
        MediaScanner::new(
            Arc::new(MediaIndex::new()),
            Arc::new(TmdbClient::new(
                "fake".to_string(),
                "http://localhost".to_string(),
                "http://localhost".to_string(),
            )),
            config,
        )
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
}
