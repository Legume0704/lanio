pub mod types;

use dashmap::DashMap;
use types::{FileInfo, IndexEntry, UnmatchedFile};

pub struct MediaIndex {
    entries: DashMap<String, IndexEntry>,
    unmatched: DashMap<String, UnmatchedFile>,
}

impl MediaIndex {
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
            unmatched: DashMap::new(),
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

    pub fn insert_unmatched(&self, file: UnmatchedFile) {
        self.unmatched.insert(file.id.clone(), file);
    }

    pub fn get_unmatched(&self, id: &str) -> Option<UnmatchedFile> {
        self.unmatched.get(id).map(|entry| entry.value().clone())
    }

    /// All unmatched files, newest first. Files with an unknown creation time
    /// sort last, with the filename as a stable tie-breaker.
    pub fn get_all_unmatched(&self) -> Vec<UnmatchedFile> {
        let mut files: Vec<UnmatchedFile> = self
            .unmatched
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        files.sort_by(|a, b| {
            b.created
                .cmp(&a.created)
                .then_with(|| a.title.cmp(&b.title))
        });
        files
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

        self.remove_unmatched_by_dir(dir_path);
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

        self.remove_unmatched_by_path(file_path);
    }

    pub fn remove_unmatched_by_path(&self, file_path: &std::path::Path) {
        self.unmatched.retain(|_, file| file.file_path != file_path);
    }

    pub fn remove_unmatched_by_dir(&self, dir_path: &std::path::Path) {
        self.unmatched
            .retain(|_, file| !file.file_path.starts_with(dir_path));
    }

    pub fn clear(&self) {
        self.entries.clear();
        self.unmatched.clear();
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
    use std::path::PathBuf;
    use types::{ContentType, ParsedMetadata, UnmatchedFile};

    fn movie(imdb_id: &str, path: &str) -> types::FileInfo {
        types::FileInfo {
            imdb_id: imdb_id.to_string(),
            title: imdb_id.to_string(),
            year: None,
            content_type: ContentType::Movie,
            file_path: PathBuf::from(path),
            parsed: ParsedMetadata {
                season: None,
                episode: None,
            },
            ..Default::default()
        }
    }

    fn episode(imdb_id: &str, path: &str, season: u16, ep: u16) -> types::FileInfo {
        types::FileInfo {
            imdb_id: imdb_id.to_string(),
            title: imdb_id.to_string(),
            year: None,
            content_type: ContentType::Series,
            file_path: PathBuf::from(path),
            parsed: ParsedMetadata {
                season: Some(season),
                episode: Some(ep),
            },
            ..Default::default()
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
        index.insert_episode(
            "tt0000002".to_string(),
            episode("tt0000002", "/s01e01.mkv", 1, 1),
        );
        let ep = index.get_episode("tt0000002", 1, 1).unwrap();
        assert_eq!(ep.parsed.season, Some(1));
        assert_eq!(ep.parsed.episode, Some(1));
        assert!(index.get_episode("tt0000002", 1, 2).is_none());
    }

    #[test]
    fn multiple_episodes_accumulate() {
        let index = MediaIndex::new();
        index.insert_episode(
            "tt0000003".to_string(),
            episode("tt0000003", "/s01e01.mkv", 1, 1),
        );
        index.insert_episode(
            "tt0000003".to_string(),
            episode("tt0000003", "/s01e02.mkv", 1, 2),
        );
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
        index.insert_movie(
            "tt0000005".to_string(),
            movie("tt0000005", "/media/movie.mkv"),
        );
        index.remove_by_path(&PathBuf::from("/media/movie.mkv"));
        assert!(index.get("tt0000005").is_none());
    }

    #[test]
    fn remove_by_path_removes_only_matching_episode() {
        let index = MediaIndex::new();
        index.insert_episode(
            "tt0000006".to_string(),
            episode("tt0000006", "/show/s01e01.mkv", 1, 1),
        );
        index.insert_episode(
            "tt0000006".to_string(),
            episode("tt0000006", "/show/s01e02.mkv", 1, 2),
        );
        index.remove_by_path(&PathBuf::from("/show/s01e01.mkv"));
        assert!(index.get_episode("tt0000006", 1, 1).is_none());
        assert!(index.get_episode("tt0000006", 1, 2).is_some());
    }

    #[test]
    fn remove_by_dir_removes_only_files_under_dir() {
        let index = MediaIndex::new();
        index.insert_movie(
            "tt0000007".to_string(),
            movie("tt0000007", "/media/movies/a.mkv"),
        );
        index.insert_movie(
            "tt0000008".to_string(),
            movie("tt0000008", "/media/other/b.mkv"),
        );
        index.remove_by_dir(&PathBuf::from("/media/movies"));
        assert!(index.get("tt0000007").is_none());
        assert!(index.get("tt0000008").is_some());
    }

    fn unmatched(id: &str, path: &str, created: Option<u64>) -> UnmatchedFile {
        UnmatchedFile {
            id: id.to_string(),
            title: path.rsplit('/').next().unwrap_or(path).to_string(),
            file_path: PathBuf::from(path),
            created,
        }
    }

    #[test]
    fn insert_and_get_unmatched() {
        let index = MediaIndex::new();
        index.insert_unmatched(unmatched("lanio-a", "/media/recording.ts", Some(100)));

        let file = index.get_unmatched("lanio-a").unwrap();
        assert_eq!(file.title, "recording.ts");
        assert_eq!(file.created, Some(100));
        assert!(index.get_unmatched("lanio-b").is_none());
    }

    #[test]
    fn unmatched_files_are_newest_first() {
        let index = MediaIndex::new();
        index.insert_unmatched(unmatched("lanio-a", "/media/old.ts", Some(100)));
        index.insert_unmatched(unmatched("lanio-b", "/media/new.ts", Some(300)));
        index.insert_unmatched(unmatched("lanio-c", "/media/middle.ts", Some(200)));

        let ids: Vec<String> = index
            .get_all_unmatched()
            .into_iter()
            .map(|file| file.id)
            .collect();
        assert_eq!(ids, vec!["lanio-b", "lanio-c", "lanio-a"]);
    }

    #[test]
    fn unmatched_files_without_a_created_time_sort_last() {
        let index = MediaIndex::new();
        index.insert_unmatched(unmatched("lanio-a", "/media/old.ts", None));
        index.insert_unmatched(unmatched("lanio-b", "/media/new.ts", Some(300)));

        let ids: Vec<String> = index
            .get_all_unmatched()
            .into_iter()
            .map(|file| file.id)
            .collect();
        assert_eq!(ids, vec!["lanio-b", "lanio-a"]);
    }

    #[test]
    fn remove_by_path_removes_matching_unmatched_file() {
        let index = MediaIndex::new();
        index.insert_unmatched(unmatched("lanio-a", "/media/recording.ts", Some(100)));
        index.remove_by_path(&PathBuf::from("/media/recording.ts"));
        assert!(index.get_unmatched("lanio-a").is_none());
    }

    #[test]
    fn remove_by_dir_removes_unmatched_files_under_dir() {
        let index = MediaIndex::new();
        index.insert_unmatched(unmatched("lanio-a", "/media/recordings/a.ts", Some(100)));
        index.insert_unmatched(unmatched("lanio-b", "/media/other/b.ts", Some(100)));
        index.remove_by_dir(&PathBuf::from("/media/recordings"));
        assert!(index.get_unmatched("lanio-a").is_none());
        assert!(index.get_unmatched("lanio-b").is_some());
    }

    #[test]
    fn clear_removes_unmatched_files() {
        let index = MediaIndex::new();
        index.insert_unmatched(unmatched("lanio-a", "/media/recording.ts", Some(100)));
        index.clear();
        assert!(index.get_all_unmatched().is_empty());
    }
}
