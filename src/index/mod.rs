pub mod types;

use dashmap::DashMap;
use types::{FileInfo, IndexEntry, IndexStats};

pub struct MediaIndex {
    entries: DashMap<String, IndexEntry>,
}

impl MediaIndex {
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use types::{ContentType, ParsedMetadata};
    use std::path::PathBuf;

    fn movie(imdb_id: &str, path: &str) -> types::FileInfo {
        types::FileInfo {
            imdb_id: imdb_id.to_string(),
            title: imdb_id.to_string(),
            year: None,
            content_type: ContentType::Movie,
            file_path: PathBuf::from(path),
            parsed: ParsedMetadata { season: None, episode: None },
            poster: None,
        }
    }

    fn episode(imdb_id: &str, path: &str, season: u16, ep: u16) -> types::FileInfo {
        types::FileInfo {
            imdb_id: imdb_id.to_string(),
            title: imdb_id.to_string(),
            year: None,
            content_type: ContentType::Series,
            file_path: PathBuf::from(path),
            parsed: ParsedMetadata { season: Some(season), episode: Some(ep) },
            poster: None,
        }
    }

    #[test]
    fn insert_and_get_movie() {
        let index = MediaIndex::new();
        index.insert_movie("tt0000001".to_string(), movie("tt0000001", "/a.mkv"));
        let entry = index.get("tt0000001").unwrap();
        assert!(matches!(entry, IndexEntry::Movie(_)));
        assert!(index.get("tt0000002").is_none());
    }

    #[test]
    fn insert_and_get_episode() {
        let index = MediaIndex::new();
        index.insert_episode("tt0000002".to_string(), episode("tt0000002", "/s01e01.mkv", 1, 1));
        let ep = index.get_episode("tt0000002", 1, 1).unwrap();
        assert_eq!(ep.parsed.season, Some(1));
        assert_eq!(ep.parsed.episode, Some(1));
        assert!(index.get_episode("tt0000002", 1, 2).is_none());
    }

    #[test]
    fn multiple_episodes_accumulate() {
        let index = MediaIndex::new();
        index.insert_episode("tt0000003".to_string(), episode("tt0000003", "/s01e01.mkv", 1, 1));
        index.insert_episode("tt0000003".to_string(), episode("tt0000003", "/s01e02.mkv", 1, 2));
        assert!(index.get_episode("tt0000003", 1, 1).is_some());
        assert!(index.get_episode("tt0000003", 1, 2).is_some());
    }

    #[test]
    fn remove_movie() {
        let index = MediaIndex::new();
        index.insert_movie("tt0000004".to_string(), movie("tt0000004", "/a.mkv"));
        index.remove_movie("tt0000004");
        assert!(index.get("tt0000004").is_none());
    }

    #[test]
    fn remove_by_path_movie() {
        let index = MediaIndex::new();
        index.insert_movie("tt0000005".to_string(), movie("tt0000005", "/media/movie.mkv"));
        index.remove_by_path(&PathBuf::from("/media/movie.mkv"));
        assert!(index.get("tt0000005").is_none());
    }

    #[test]
    fn remove_by_path_removes_only_matching_episode() {
        let index = MediaIndex::new();
        index.insert_episode("tt0000006".to_string(), episode("tt0000006", "/show/s01e01.mkv", 1, 1));
        index.insert_episode("tt0000006".to_string(), episode("tt0000006", "/show/s01e02.mkv", 1, 2));
        index.remove_by_path(&PathBuf::from("/show/s01e01.mkv"));
        assert!(index.get_episode("tt0000006", 1, 1).is_none());
        assert!(index.get_episode("tt0000006", 1, 2).is_some());
    }

    #[test]
    fn remove_by_dir_removes_only_files_under_dir() {
        let index = MediaIndex::new();
        index.insert_movie("tt0000007".to_string(), movie("tt0000007", "/media/movies/a.mkv"));
        index.insert_movie("tt0000008".to_string(), movie("tt0000008", "/media/other/b.mkv"));
        index.remove_by_dir(&PathBuf::from("/media/movies"));
        assert!(index.get("tt0000007").is_none());
        assert!(index.get("tt0000008").is_some());
    }

    #[test]
    fn stats_counts_correctly() {
        let index = MediaIndex::new();
        index.insert_movie("tt0000009".to_string(), movie("tt0000009", "/a.mkv"));
        index.insert_movie("tt0000010".to_string(), movie("tt0000010", "/b.mkv"));
        index.insert_episode("tt0000011".to_string(), episode("tt0000011", "/s01e01.mkv", 1, 1));
        index.insert_episode("tt0000011".to_string(), episode("tt0000011", "/s01e02.mkv", 1, 2));
        let s = index.stats();
        assert_eq!(s.movies, 2);
        assert_eq!(s.series, 1);
        assert_eq!(s.episodes, 2);
    }

    #[test]
    fn clear_empties_index() {
        let index = MediaIndex::new();
        index.insert_movie("tt0000012".to_string(), movie("tt0000012", "/a.mkv"));
        index.clear();
        assert!(index.get("tt0000012").is_none());
        assert_eq!(index.stats().movies, 0);
    }
}
