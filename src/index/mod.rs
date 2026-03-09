pub mod types;

use dashmap::DashMap;
use std::sync::Arc;
use types::{FileInfo, IndexEntry, IndexStats};

pub struct MediaIndex {
    entries: Arc<DashMap<String, IndexEntry>>,
}

impl MediaIndex {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(DashMap::new()),
        }
    }

    pub fn get(&self, imdb_id: &str) -> Option<IndexEntry> {
        self.entries.get(imdb_id).map(|v| v.clone())
    }

    pub fn get_episode(&self, imdb_id: &str, season: u16, episode: u16) -> Option<FileInfo> {
        if let Some(entry) = self.entries.get(imdb_id) {
            match entry.value() {
                IndexEntry::Series(episodes) => episodes
                    .iter()
                    .find(|ep| {
                        ep.parsed.season == Some(season) && ep.parsed.episode == Some(episode)
                    })
                    .cloned(),
                IndexEntry::Movie(_) => None,
            }
        } else {
            None
        }
    }

    pub fn insert_movie(&self, imdb_id: String, file_info: FileInfo) {
        self.entries.insert(imdb_id, IndexEntry::Movie(file_info));
    }

    pub fn insert_episode(&self, imdb_id: String, file_info: FileInfo) {
        self.entries
            .entry(imdb_id.clone())
            .and_modify(|entry| {
                if let IndexEntry::Series(episodes) = entry {
                    episodes.push(file_info.clone());
                }
            })
            .or_insert_with(|| IndexEntry::Series(vec![file_info]));
    }

    pub fn remove_movie(&self, imdb_id: &str) {
        self.entries.remove(imdb_id);
    }

    pub fn remove_episode(&self, imdb_id: &str, file_path: &std::path::Path) {
        if let Some(mut entry) = self.entries.get_mut(imdb_id) {
            if let IndexEntry::Series(episodes) = entry.value_mut() {
                episodes.retain(|ep| ep.file_path != file_path);
                // If no episodes left, we'll remove the entry after dropping the lock
                if episodes.is_empty() {
                    drop(entry);
                    self.entries.remove(imdb_id);
                }
            }
        }
    }

    pub fn remove_by_dir(&self, dir_path: &std::path::Path) {
        let movie_removals: Vec<String> = self
            .entries
            .iter()
            .filter_map(|entry| match entry.value() {
                IndexEntry::Movie(file_info) if file_info.file_path.starts_with(dir_path) => {
                    Some(entry.key().clone())
                }
                _ => None,
            })
            .collect();

        for imdb_id in movie_removals {
            self.remove_movie(&imdb_id);
        }

        let series_removals: Vec<(String, std::path::PathBuf)> = self
            .entries
            .iter()
            .filter_map(|entry| match entry.value() {
                IndexEntry::Series(episodes) => {
                    let paths: Vec<_> = episodes
                        .iter()
                        .filter(|ep| ep.file_path.starts_with(dir_path))
                        .map(|ep| ep.file_path.clone())
                        .collect();
                    if paths.is_empty() {
                        None
                    } else {
                        Some((entry.key().clone(), paths))
                    }
                }
                _ => None,
            })
            .flat_map(|(id, paths)| paths.into_iter().map(move |p| (id.clone(), p)))
            .collect();

        for (imdb_id, file_path) in series_removals {
            self.remove_episode(&imdb_id, &file_path);
        }
    }

    pub fn remove_by_path(&self, file_path: &std::path::Path) {
        let movie_ids: Vec<String> = self
            .entries
            .iter()
            .filter_map(|entry| match entry.value() {
                IndexEntry::Movie(file_info) if file_info.file_path == file_path => {
                    Some(entry.key().clone())
                }
                _ => None,
            })
            .collect();

        for imdb_id in movie_ids {
            self.remove_movie(&imdb_id);
        }

        let series_ids: Vec<String> = self
            .entries
            .iter()
            .filter_map(|entry| match entry.value() {
                IndexEntry::Series(episodes)
                    if episodes.iter().any(|ep| ep.file_path == file_path) =>
                {
                    Some(entry.key().clone())
                }
                _ => None,
            })
            .collect();

        for imdb_id in series_ids {
            self.remove_episode(&imdb_id, file_path);
        }
    }

    pub fn clear(&self) {
        self.entries.clear();
    }

    pub fn stats(&self) -> IndexStats {
        let mut movies = 0;
        let mut series = 0;
        let mut episodes = 0;

        for entry in self.entries.iter() {
            match entry.value() {
                IndexEntry::Movie(_) => movies += 1,
                IndexEntry::Series(eps) => {
                    series += 1;
                    episodes += eps.len();
                }
            }
        }

        IndexStats {
            movies,
            series,
            episodes,
        }
    }

    pub fn get_all_movies(&self) -> Vec<(String, FileInfo)> {
        self.entries
            .iter()
            .filter_map(|entry| match entry.value() {
                IndexEntry::Movie(file_info) => Some((entry.key().clone(), file_info.clone())),
                IndexEntry::Series(_) => None,
            })
            .collect()
    }

    pub fn get_all_series(&self) -> Vec<(String, FileInfo)> {
        self.entries
            .iter()
            .filter_map(|entry| match entry.value() {
                IndexEntry::Series(episodes) => {
                    // Return first episode to get series metadata
                    episodes
                        .first()
                        .map(|file_info| (entry.key().clone(), file_info.clone()))
                }
                IndexEntry::Movie(_) => None,
            })
            .collect()
    }
}

impl Default for MediaIndex {
    fn default() -> Self {
        Self::new()
    }
}
