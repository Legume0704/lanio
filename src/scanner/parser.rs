use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Extract year: 1900-2099
    static ref YEAR_REGEX: Regex = Regex::new(r"\b(19\d{2}|20\d{2})\b").unwrap();

    // Season/Episode: S01E01 or 1x01
    static ref SEASON_EPISODE_REGEX: Regex = Regex::new(r"(?i)s(\d{1,2})e(\d{1,2})").unwrap();
    static ref ALTERNATE_SE_REGEX: Regex = Regex::new(r"(?i)(\d{1,2})x(\d{1,2})").unwrap();

    // IMDb ID: tt1234567 or tt12345678
    static ref IMDB_ID_REGEX: Regex = Regex::new(r"(?i)tt\d{7,8}").unwrap();

    // Clean up: empty brackets, file extensions, orphaned brackets
    static ref EMPTY_BRACKETS: Regex = Regex::new(r"\(\s*\)|\[\s*\]").unwrap();
    static ref ORPHANED_BRACKETS: Regex = Regex::new(r"[\(\[\]\)]").unwrap();
    static ref EXTENSION_REGEX: Regex = Regex::new(
        r"\.(?i)(mkv|mp4|avi|mov|wmv|flv|webm|m4v|mpg|mpeg|m2ts|ts|vob)$"
    ).unwrap();
    static ref SEPARATOR_REGEX: Regex = Regex::new(r"[\.\-_]+").unwrap();
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

    // Extract year and its position
    let year_match = YEAR_REGEX.find(&working);
    let year = year_match.as_ref().and_then(|m| m.as_str().parse::<u16>().ok());

    // Extract season/episode and its position
    let se_match = SEASON_EPISODE_REGEX.find(&working)
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
}
