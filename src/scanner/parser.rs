use lazy_static::lazy_static;
use regex::Regex;
use std::path::Path;

lazy_static! {
    // Extract year: 1900-2099
    static ref YEAR_REGEX: Regex = Regex::new(r"\b(19\d{2}|20\d{2})\b").unwrap();

    // Season/Episode: S01E01 or 1x01
    static ref SEASON_EPISODE_REGEX: Regex = Regex::new(r"(?i)s(\d{1,2})e(\d{1,2})").unwrap();
    static ref ALTERNATE_SE_REGEX: Regex = Regex::new(r"(?i)(\d{1,2})x(\d{1,2})").unwrap();

    // Season folder: "Season 01", "season 1", "Saison 3"
    static ref SEASON_DIR_REGEX: Regex =
        Regex::new(r"(?i)^(?:season|saison|staffel|temporada)\s*(\d{1,3})$").unwrap();

    // Season folder in the bare form: "S01", "s2"
    static ref BARE_SEASON_DIR_REGEX: Regex = Regex::new(r"(?i)^s\s*(\d{1,3})$").unwrap();

    // Specials folder, which is season 0 in the S00Exx filename form
    static ref SPECIALS_DIR_REGEX: Regex = Regex::new(r"(?i)^(?:specials?|extras?)$").unwrap();

    // Disc folder inside a season: "Disc 1", "Part 2", "CD 3"
    static ref DISC_DIR_REGEX: Regex = Regex::new(r"(?i)^(?:disc|disk|cd|dvd|part|pt)\s*\d*$").unwrap();

    // Season-relative episode number, at either end of a filename:
    // "01 - Pilot.mkv" or "Pilot.01.mkv"
    static ref LEADING_EPISODE_REGEX: Regex = Regex::new(r"^(\d{1,3})\s*(?:[-._]|\s)").unwrap();

    // The first 1-3 digit token bounded by a separator or bracket. Bounding on
    // both sides keeps "Pilot (01) 1080p" at 1 while rejecting "Pilot.720p",
    // whose number runs into the "p".
    static ref TRAILING_EPISODE_REGEX: Regex =
        Regex::new(r"(?:^|[._\-\s()\[\]])(\d{1,3})(?:[._\-\s()\[\]]|$)").unwrap();

    // IMDb ID: tt1234567 or tt12345678
    static ref IMDB_ID_REGEX: Regex = Regex::new(r"(?i)tt\d{7,8}").unwrap();

    // Clean up: empty brackets, file extensions, orphaned brackets
    static ref EMPTY_BRACKETS: Regex = Regex::new(r"\(\s*\)|\[\s*\]").unwrap();
    static ref ORPHANED_BRACKETS: Regex = Regex::new(r"[\(\[\]\)]").unwrap();
    static ref EXTENSION_REGEX: Regex = Regex::new(
        r"\.(?i)(mkv|mp4|avi|mov|wmv|flv|webm|m4v|mpg|mpeg|m2ts|ts|vob)$"
    ).unwrap();
    static ref SEPARATOR_REGEX: Regex = Regex::new(r"[\.\-_]+").unwrap();
    static ref ALPHANUMERIC: Regex = Regex::new(r"[0-9A-Za-z]").unwrap();
}

#[derive(Debug, Clone)]
pub struct ParsedFilename {
    pub title: String,
    pub year: Option<u16>,
    pub season: Option<u16>,
    pub episode: Option<u16>,
    pub is_series: bool,
}

pub fn parse_filename(filename: &str) -> ParsedFilename {
    let mut working = filename.to_string();

    // An IMDb ID is metadata, never part of the title.
    working = IMDB_ID_REGEX.replace_all(&working, " ").to_string();

    // Extract year and its position. A number only counts as a year when there
    // is title content in front of it — otherwise a numeric title like "2012"
    // is consumed as the year and the title parses to nothing.
    let year_match = find_year(&working);
    let year = year_match
        .as_ref()
        .and_then(|m| m.as_str().parse::<u16>().ok());

    // Extract season/episode and its position
    let se_match = SEASON_EPISODE_REGEX
        .find(&working)
        .or_else(|| ALTERNATE_SE_REGEX.find(&working));

    let (season, episode) = if let Some(caps) = SEASON_EPISODE_REGEX.captures(&working) {
        let s = caps.get(1).and_then(|m| m.as_str().parse::<u16>().ok());
        let e = caps.get(2).and_then(|m| m.as_str().parse::<u16>().ok());
        (s, e)
    } else if let Some(caps) = ALTERNATE_SE_REGEX.captures(&working) {
        let s = caps.get(1).and_then(|m| m.as_str().parse::<u16>().ok());
        let e = caps.get(2).and_then(|m| m.as_str().parse::<u16>().ok());
        (s, e)
    } else {
        (None, None)
    };

    let is_series = season.is_some() || episode.is_some();

    // Extract title: everything before the year OR season/episode indicator (whichever comes first)
    let cutoff_pos = match (year_match, se_match) {
        (Some(y), Some(se)) => Some(y.start().min(se.start())),
        (Some(y), None) => Some(y.start()),
        (None, Some(se)) => Some(se.start()),
        (None, None) => None,
    };

    if let Some(pos) = cutoff_pos {
        working = working[..pos].to_string();
    }

    working = EXTENSION_REGEX.replace(&working, "").to_string();

    // Clean up title
    working = EMPTY_BRACKETS.replace_all(&working, "").to_string();
    working = ORPHANED_BRACKETS.replace_all(&working, "").to_string();
    working = SEPARATOR_REGEX.replace_all(&working, " ").to_string();
    let title = working.trim().to_string();

    ParsedFilename {
        title,
        year,
        season,
        episode,
        is_series,
    }
}

pub fn extract_imdb_id(text: &str) -> Option<String> {
    IMDB_ID_REGEX.find(text).map(|m| m.as_str().to_lowercase())
}

/// What a directory between a media file and the library root declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirKind {
    /// A season folder. `Specials` is 0, and disc folders inside one join it.
    Season(u16),
    /// Groups a season's files without naming the show, e.g. `Disc 1`.
    Container,
    /// Any other folder — a candidate for the show name and year.
    Show,
}

/// The season a folder declares: `Season 01`, `S02`, `Saison 3`, or 0 for
/// `Specials`/`Extras` to match the `S00Exx` filename form.
pub fn parse_season_dir(name: &str) -> Option<u16> {
    let name = name.trim();

    if let Some(caps) = SEASON_DIR_REGEX
        .captures(name)
        .or_else(|| BARE_SEASON_DIR_REGEX.captures(name))
    {
        return caps.get(1).and_then(|m| m.as_str().parse::<u16>().ok());
    }

    if SPECIALS_DIR_REGEX.is_match(name) {
        return Some(0);
    }

    None
}

/// A folder grouping a season's files without naming the show.
pub fn is_disc_dir(name: &str) -> bool {
    DISC_DIR_REGEX.is_match(name.trim())
}

/// A season-relative episode number from either end of a filename:
/// "01 - Pilot.mkv" or "Pilot.01.mkv". Only meaningful inside a season folder,
/// where a bare number is the episode rather than part of the title.
pub fn parse_season_episode(filename: &str) -> Option<u16> {
    let filename = filename.trim();

    LEADING_EPISODE_REGEX
        .captures(filename)
        .or_else(|| TRAILING_EPISODE_REGEX.captures(filename))
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse::<u16>().ok())
}

/// The folders between `file_path` and the library root, nearest first.
/// Stops before `media_path`, and is depth-capped because `media_path` need not
/// be a literal ancestor of a scanned file.
pub fn dir_chain<'a>(file_path: &'a Path, media_path: &Path) -> Vec<(&'a str, DirKind)> {
    const MAX_DEPTH: usize = 4;

    let mut chain: Vec<(&str, DirKind)> = file_path
        .parent()
        .into_iter()
        .flat_map(Path::ancestors)
        .take_while(|dir| *dir != media_path)
        .take(MAX_DEPTH)
        .filter_map(|dir| dir.file_name().and_then(|n| n.to_str()))
        .map(|name| match parse_season_dir(name) {
            Some(season) => (name, DirKind::Season(season)),
            None if is_disc_dir(name) => (name, DirKind::Container),
            None => (name, DirKind::Show),
        })
        .collect();

    // A season folder owns the file only if everything between them is one of
    // its disc folders, so `Season 1/Show (2008)/ep.mkv` keeps its show folder
    // and its season folder is demoted to a plain grouping folder.
    if let Some(index) = chain
        .iter()
        .position(|(_, kind)| matches!(kind, DirKind::Season(_)))
    {
        let only_discs_below = chain[..index]
            .iter()
            .all(|(_, kind)| matches!(kind, DirKind::Container));

        if let DirKind::Season(season) = chain[index].1 {
            if only_discs_below {
                for entry in chain.iter_mut().take(index) {
                    entry.1 = DirKind::Season(season);
                }
            } else {
                chain[index].1 = DirKind::Container;
            }
        }
    }

    chain
}

/// First 1900-2099 number that has at least one alphanumeric character in
/// front of it, so a leading numeric title is not mistaken for a release year.
fn find_year(text: &str) -> Option<regex::Match<'_>> {
    YEAR_REGEX
        .find_iter(text)
        .find(|m| ALPHANUMERIC.is_match(&text[..m.start()]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_movie() {
        let parsed = parse_filename("Inception.2010.1080p.BluRay.mkv");
        assert_eq!(parsed.title, "Inception");
        assert_eq!(parsed.year, Some(2010));
        assert!(!parsed.is_series);
    }

    #[test]
    fn test_parse_series() {
        let parsed = parse_filename("Breaking.Bad.S01E01.mkv");
        assert_eq!(parsed.title, "Breaking Bad");
        assert_eq!(parsed.season, Some(1));
        assert_eq!(parsed.episode, Some(1));
        assert!(parsed.is_series);
    }

    #[test]
    fn test_parse_series_with_quality() {
        let parsed = parse_filename("The.Mandalorian.S02E08.1080p.WEB-DL.mkv");
        assert_eq!(parsed.title, "The Mandalorian");
        assert_eq!(parsed.season, Some(2));
        assert_eq!(parsed.episode, Some(8));
        assert!(parsed.is_series);
    }

    #[test]
    fn test_parse_series_alternate_format() {
        let parsed = parse_filename("Game.of.Thrones.3x09.720p.HDTV.mkv");
        assert_eq!(parsed.title, "Game of Thrones");
        assert_eq!(parsed.season, Some(3));
        assert_eq!(parsed.episode, Some(9));
        assert!(parsed.is_series);
    }

    #[test]
    fn test_extract_imdb_id() {
        let id = extract_imdb_id("Movie.Name.tt1234567.mkv");
        assert_eq!(id, Some("tt1234567".to_string()));
    }

    #[test]
    fn test_clean_title() {
        let parsed = parse_filename("The Bob's Burgers Movie (2022).mkv");
        assert_eq!(parsed.title, "The Bob's Burgers Movie");
        assert_eq!(parsed.year, Some(2022));
    }

    #[test]
    fn test_complex_quality_stripping() {
        let parsed = parse_filename("Star.Wars.Episode.I.The.Phantom.Menace.1999.2160p.HDR.Disney.WEBRip.DTS-HD.MA.6.1.x265-TrollUHD.mkv");
        assert_eq!(parsed.title, "Star Wars Episode I The Phantom Menace");
        assert_eq!(parsed.year, Some(1999));
        assert!(!parsed.is_series);
    }

    #[test]
    fn test_complex_quality_stripping_2() {
        let parsed = parse_filename("Everything.Everywhere.All.at.Once.2022.2160p.UHD.BluRay.x265.10bit.HDR.DTS-HD.MA.TrueHD.7.1.Atmos-SWTYBLZ.mkv");
        assert_eq!(parsed.title, "Everything Everywhere All at Once");
        assert_eq!(parsed.year, Some(2022));
        assert!(!parsed.is_series);
    }

    #[test]
    fn test_title_before_year() {
        let parsed = parse_filename("The.Matrix.1999.1080p.BluRay.x264-GROUP.mkv");
        assert_eq!(parsed.title, "The Matrix");
        assert_eq!(parsed.year, Some(1999));
    }

    #[test]
    fn test_numeric_title_with_year() {
        let parsed = parse_filename("2012.2009.1080p.BluRay.mkv");
        assert_eq!(parsed.title, "2012");
        assert_eq!(parsed.year, Some(2009));
        assert!(!parsed.is_series);
    }

    #[test]
    fn test_numeric_title_in_parentheses() {
        let parsed = parse_filename("2012 (2009).mkv");
        assert_eq!(parsed.title, "2012");
        assert_eq!(parsed.year, Some(2009));
    }

    #[test]
    fn test_numeric_title_without_release_year() {
        let parsed = parse_filename("2012.mkv");
        assert_eq!(parsed.title, "2012");
        assert_eq!(parsed.year, None);
    }

    #[test]
    fn test_numeric_title_with_imdb_id() {
        let parsed = parse_filename("2012.tt1099212.mkv");
        assert_eq!(parsed.title, "2012");
        assert_eq!(parsed.year, None);
    }

    #[test]
    fn test_numeric_title_within_longer_title() {
        let parsed = parse_filename("2001.A.Space.Odyssey.1968.1080p.mkv");
        assert_eq!(parsed.title, "2001 A Space Odyssey");
        assert_eq!(parsed.year, Some(1968));
    }

    #[test]
    fn test_bracketed_numeric_title() {
        let parsed = parse_filename("(2012).2009.mkv");
        assert_eq!(parsed.title, "2012");
        assert_eq!(parsed.year, Some(2009));
    }

    #[test]
    fn test_four_digit_numeric_title_alone_is_not_a_year() {
        let parsed = parse_filename("1917.mkv");
        assert_eq!(parsed.title, "1917");
        assert_eq!(parsed.year, None);
    }

    #[test]
    fn test_imdb_id_is_stripped_from_title() {
        let parsed = parse_filename("Movie.Name.tt1234567.mkv");
        assert_eq!(parsed.title, "Movie Name");
    }

    #[test]
    fn season_dir_numbers() {
        for (name, expected) in [
            ("Season 1", Some(1)),
            ("Season 01", Some(1)),
            ("Season 001", Some(1)),
            ("season  12", Some(12)),
            ("SEASON 7", Some(7)),
            ("S01", Some(1)),
            ("s2", Some(2)),
            ("Saison 3", Some(3)),
            ("Staffel 4", Some(4)),
            ("Temporada 10", Some(10)),
        ] {
            assert_eq!(parse_season_dir(name), expected, "for folder {name:?}");
        }
    }

    #[test]
    fn specials_dir_is_season_zero() {
        for name in ["Specials", "specials", "Extras", "Extra", "Special"] {
            assert_eq!(parse_season_dir(name), Some(0), "for folder {name:?}");
        }
        assert_eq!(parse_season_dir("Season 0"), Some(0));
    }

    #[test]
    fn show_folders_are_not_season_dirs() {
        for name in [
            "Season One",
            "Seasons",
            "TV Series",
            "Series",
            "Breaking Bad (2008)",
            "S01E01",
            "",
        ] {
            assert_eq!(parse_season_dir(name), None, "for folder {name:?}");
        }
    }

    #[test]
    fn disc_folders_are_containers() {
        for name in ["Disc 1", "disc2", "Part 2", "CD 3", "DVD", "Disk 1"] {
            assert!(is_disc_dir(name), "for folder {name:?}");
        }
        for name in ["Discworld", "Breaking Bad (2008)", "Parting"] {
            assert!(!is_disc_dir(name), "for folder {name:?}");
        }
    }

    #[test]
    fn season_episode_numbers() {
        for (name, expected) in [
            // Leading
            ("01 - Pilot.mkv", Some(1)),
            ("1 - Pilot.mkv", Some(1)),
            ("01. Pilot.mkv", Some(1)),
            ("01_Pilot.mkv", Some(1)),
            ("01 Pilot.mkv", Some(1)),
            ("12 - The Twelve.mkv", Some(12)),
            ("007 - Agent.mkv", Some(7)),
            ("1 - Pilot", Some(1)),
            ("01 - Pilot.1080p.mkv", Some(1)),
            // Trailing, including behind release tags
            ("Pilot.01.mkv", Some(1)),
            ("Pilot 01.mkv", Some(1)),
            ("Pilot.12.mkv", Some(12)),
            ("Episode 1.mkv", Some(1)),
            ("Pilot.01", Some(1)),
            ("Pilot.01.1080p.mkv", Some(1)),
            ("Pilot.01.2160p.WEB-DL.x265.mkv", Some(1)),
            ("Pilot.01.10bit.WEB-DL.DDP5.1.H.265.mkv", Some(1)),
            // Bracketed, which is a common manual-rename convention
            ("Pilot (01) 1080p.mkv", Some(1)),
            ("Pilot (01).mkv", Some(1)),
            ("(01) Pilot.mkv", Some(1)),
            ("[01] Pilot.mkv", Some(1)),
            ("Pilot [12].mkv", Some(12)),
            ("Show.S01E01 (1080p).mkv", None),
            // The first token is the episode, not a later count
            ("Episode 5 of 10.mkv", Some(5)),
        ] {
            assert_eq!(parse_season_episode(name), expected, "for file {name:?}");
        }
    }

    #[test]
    fn numeric_titles_and_quality_tags_are_not_episode_numbers() {
        for name in [
            "1917.mkv",
            "2012.mkv",
            "1080p.mkv",
            "1883.mkv",
            "Blade.Runner.2049.mkv",
            "Pilot.720p.mkv",
            "Pilot.1080p.mkv",
            "Pilot (720p).mkv",
            "Show (2010).mkv",
            "1917 (2019).mkv",
            "Blade Runner (2049).mkv",
            "Show.1080p.WEB-DL.mkv",
            "Show.S01.1080p.mkv",
            "Pilot.mkv",
        ] {
            assert_eq!(parse_season_episode(name), None, "for file {name:?}");
        }
    }

    /// A delimited 1-3 digit token is a number, not a release tag, so a bare
    /// audio channel pair inside a season folder is read as an episode number.
    /// Pinned so the trade-off stays visible.
    #[test]
    fn audio_channel_pair_is_read_as_an_episode_number() {
        assert_eq!(parse_season_episode("Pilot.5.1.mkv"), Some(5));
    }

    #[test]
    fn bare_leading_number_needs_the_caller_guard() {
        assert_eq!(parse_season_episode("24.mkv"), Some(24));
        assert_eq!(parse_season_episode("2012.1080p.mkv"), None);
    }

    #[test]
    fn dir_chain_starts_at_the_nearest_folder() {
        let file = Path::new("/media/Breaking Bad (2008)/Breaking.Bad.S01E01.mkv");
        assert_eq!(
            dir_chain(file, Path::new("/media")),
            vec![("Breaking Bad (2008)", DirKind::Show)]
        );
    }

    #[test]
    fn dir_chain_reports_season_folder_before_show_folder() {
        let file = Path::new("/media/Breaking Bad (2008)/Season 01/Show.S01E01.mkv");
        assert_eq!(
            dir_chain(file, Path::new("/media")),
            vec![
                ("Season 01", DirKind::Season(1)),
                ("Breaking Bad (2008)", DirKind::Show),
            ]
        );
    }

    #[test]
    fn disc_folder_is_folded_into_its_season() {
        let file = Path::new("/media/Breaking Bad (2008)/Season 1/Disc 1/Show.S01E01.mkv");
        assert_eq!(
            dir_chain(file, Path::new("/media")),
            vec![
                ("Disc 1", DirKind::Season(1)),
                ("Season 1", DirKind::Season(1)),
                ("Breaking Bad (2008)", DirKind::Show),
            ],
            "a disc folder belongs to the season above it, not to the show"
        );
    }

    /// A show folder above the season folder is not absorbed, which is what
    /// stops a movie filed under `Season 1/` from becoming an episode.
    #[test]
    fn show_folder_above_season_folder_is_not_absorbed() {
        let file = Path::new("/media/Season 1/Show (2008)/ep.mkv");
        assert_eq!(
            dir_chain(file, Path::new("/media")),
            vec![
                ("Show (2008)", DirKind::Show),
                ("Season 1", DirKind::Container),
            ],
            "a season folder the file sits above must not supply a season"
        );
    }

    #[test]
    fn dir_chain_never_includes_the_library_root() {
        let file = Path::new("/media/TV/Show (2008)/Season 1/ep.mkv");
        assert_eq!(
            dir_chain(file, Path::new("/media")),
            vec![
                ("Season 1", DirKind::Season(1)),
                ("Show (2008)", DirKind::Show),
                ("TV", DirKind::Show),
            ],
            "stops before MEDIA_PATH, but may look past it when MEDIA_PATH is \
             not a literal ancestor"
        );
    }

    #[test]
    fn dir_chain_is_depth_capped() {
        let file = Path::new("/a/b/c/d/e/Season 1/ep.mkv");
        assert_eq!(
            dir_chain(file, Path::new("/somewhere/else")),
            vec![
                ("Season 1", DirKind::Season(1)),
                ("e", DirKind::Show),
                ("d", DirKind::Show),
                ("c", DirKind::Show),
            ]
        );
    }
}
