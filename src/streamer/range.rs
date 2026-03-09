use crate::error::{AppError, Result};

pub fn parse_range_header(range_header: &str, file_size: u64) -> Result<(u64, u64)> {
    // Range header format: "bytes=start-end"
    if !range_header.starts_with("bytes=") {
        return Err(AppError::InvalidPath("Invalid range header".into()));
    }

    let range_str = range_header.trim_start_matches("bytes=");
    let parts: Vec<&str> = range_str.split('-').collect();

    if parts.len() != 2 {
        return Err(AppError::InvalidPath("Invalid range format".into()));
    }

    let start = parts[0]
        .parse::<u64>()
        .map_err(|_| AppError::InvalidPath("Invalid range start".into()))?;

    let end = if parts[1].is_empty() {
        file_size - 1
    } else {
        parts[1]
            .parse::<u64>()
            .map_err(|_| AppError::InvalidPath("Invalid range end".into()))?
            .min(file_size - 1)
    };

    if start > end || start >= file_size {
        return Err(AppError::InvalidPath("Invalid range values".into()));
    }

    Ok((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_range_header() {
        let file_size = 1000;

        // Full range with end
        let (start, end) = parse_range_header("bytes=0-999", file_size).unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 999);

        // Range without end
        let (start, end) = parse_range_header("bytes=100-", file_size).unwrap();
        assert_eq!(start, 100);
        assert_eq!(end, 999);

        // Partial range
        let (start, end) = parse_range_header("bytes=200-299", file_size).unwrap();
        assert_eq!(start, 200);
        assert_eq!(end, 299);
    }
}
