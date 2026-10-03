#[derive(Debug, PartialEq, Eq)]
pub enum RangeRequest {
    /// A satisfiable byte range, inclusive on both ends.
    Satisfiable(u64, u64),
    /// Syntactically valid but outside the file; respond with 416.
    Unsatisfiable,
    /// Not a single byte range we understand; serve the whole file instead.
    Ignored,
}

pub fn parse_range_header(range_header: &str, file_size: u64) -> RangeRequest {
    // Range header format: "bytes=start-end", "bytes=start-" or "bytes=-suffix_length"
    let Some(range_str) = range_header.trim().strip_prefix("bytes=") else {
        return RangeRequest::Ignored;
    };

    // Multiple ranges aren't supported; RFC 9110 allows ignoring the header.
    if range_str.contains(',') {
        return RangeRequest::Ignored;
    }

    let Some((start_str, end_str)) = range_str.trim().split_once('-') else {
        return RangeRequest::Ignored;
    };

    if start_str.is_empty() {
        // Suffix range: the last N bytes of the file
        let Ok(suffix_len) = end_str.parse::<u64>() else {
            return RangeRequest::Ignored;
        };
        if suffix_len == 0 || file_size == 0 {
            return RangeRequest::Unsatisfiable;
        }
        let start = file_size.saturating_sub(suffix_len);
        return RangeRequest::Satisfiable(start, file_size - 1);
    }

    let Ok(start) = start_str.parse::<u64>() else {
        return RangeRequest::Ignored;
    };

    if start >= file_size {
        return RangeRequest::Unsatisfiable;
    }

    let end = if end_str.is_empty() {
        file_size - 1
    } else {
        match end_str.parse::<u64>() {
            Ok(end) => end.min(file_size - 1),
            Err(_) => return RangeRequest::Ignored,
        }
    };

    if start > end {
        return RangeRequest::Ignored;
    }

    RangeRequest::Satisfiable(start, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use RangeRequest::*;

    #[test]
    fn test_parse_range_header() {
        let file_size = 1000;

        // Full range with end
        assert_eq!(
            parse_range_header("bytes=0-999", file_size),
            Satisfiable(0, 999)
        );

        // Range without end
        assert_eq!(
            parse_range_header("bytes=100-", file_size),
            Satisfiable(100, 999)
        );

        // Partial range
        assert_eq!(
            parse_range_header("bytes=200-299", file_size),
            Satisfiable(200, 299)
        );

        // End past EOF is clamped
        assert_eq!(
            parse_range_header("bytes=900-5000", file_size),
            Satisfiable(900, 999)
        );
    }

    #[test]
    fn test_suffix_range() {
        assert_eq!(
            parse_range_header("bytes=-100", 1000),
            Satisfiable(900, 999)
        );
        assert_eq!(parse_range_header("bytes=-5000", 1000), Satisfiable(0, 999));
        assert_eq!(parse_range_header("bytes=-0", 1000), Unsatisfiable);
    }

    #[test]
    fn test_unsatisfiable_range() {
        assert_eq!(parse_range_header("bytes=1000-", 1000), Unsatisfiable);
        assert_eq!(parse_range_header("bytes=2000-3000", 1000), Unsatisfiable);
        assert_eq!(parse_range_header("bytes=0-", 0), Unsatisfiable);
    }

    #[test]
    fn test_ignored_range() {
        assert_eq!(parse_range_header("items=0-10", 1000), Ignored);
        assert_eq!(parse_range_header("bytes=0-10,20-30", 1000), Ignored);
        assert_eq!(parse_range_header("bytes=abc-", 1000), Ignored);
        assert_eq!(parse_range_header("bytes=500-100", 1000), Ignored);
    }
}
