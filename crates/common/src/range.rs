use thiserror::Error;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

impl ByteRange {
    pub fn len(self) -> u64 {
        self.end - self.start + 1
    }

    pub fn is_empty(self) -> bool {
        false
    }

    pub fn to_header_value(self) -> String {
        format!("bytes={}-{}", self.start, self.end)
    }

    pub fn to_content_range(self, total_size: u64) -> String {
        format!("bytes {}-{}/{}", self.start, self.end, total_size)
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum RangeError {
    #[error("malformed range header")]
    Malformed,
    #[error("multiple ranges are not supported")]
    MultipleRanges,
    #[error("range is unsatisfiable")]
    Unsatisfiable,
}

pub fn parse_range_header(header: &str, total_size: u64) -> Result<ByteRange, RangeError> {
    if total_size == 0 {
        return Err(RangeError::Unsatisfiable);
    }

    let raw = header
        .strip_prefix("bytes=")
        .ok_or(RangeError::Malformed)?
        .trim();

    if raw.contains(',') {
        return Err(RangeError::MultipleRanges);
    }

    let (start_raw, end_raw) = raw.split_once('-').ok_or(RangeError::Malformed)?;
    if start_raw.is_empty() {
        let suffix = end_raw.parse::<u64>().map_err(|_| RangeError::Malformed)?;
        if suffix == 0 {
            return Err(RangeError::Unsatisfiable);
        }

        let len = suffix.min(total_size);
        return Ok(ByteRange {
            start: total_size - len,
            end: total_size - 1,
        });
    }

    let start = start_raw
        .parse::<u64>()
        .map_err(|_| RangeError::Malformed)?;
    if start >= total_size {
        return Err(RangeError::Unsatisfiable);
    }

    let end = if end_raw.is_empty() {
        total_size - 1
    } else {
        end_raw.parse::<u64>().map_err(|_| RangeError::Malformed)?
    };

    if end < start {
        return Err(RangeError::Malformed);
    }

    Ok(ByteRange {
        start,
        end: end.min(total_size - 1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_explicit_range() {
        let range = parse_range_header("bytes=0-9", 100).unwrap();
        assert_eq!(range, ByteRange { start: 0, end: 9 });
        assert_eq!(range.len(), 10);
        assert_eq!(range.to_header_value(), "bytes=0-9");
        assert_eq!(range.to_content_range(100), "bytes 0-9/100");
    }

    #[test]
    fn parses_open_ended_and_suffix_ranges() {
        assert_eq!(
            parse_range_header("bytes=10-", 100).unwrap(),
            ByteRange { start: 10, end: 99 }
        );
        assert_eq!(
            parse_range_header("bytes=-10", 100).unwrap(),
            ByteRange { start: 90, end: 99 }
        );
    }

    #[test]
    fn rejects_invalid_ranges() {
        assert_eq!(
            parse_range_header("0-10", 100).unwrap_err(),
            RangeError::Malformed
        );
        assert_eq!(
            parse_range_header("bytes=10-5", 100).unwrap_err(),
            RangeError::Malformed
        );
        assert_eq!(
            parse_range_header("bytes=10-20,30-40", 100).unwrap_err(),
            RangeError::MultipleRanges
        );
        assert_eq!(
            parse_range_header("bytes=100-", 100).unwrap_err(),
            RangeError::Unsatisfiable
        );
    }
}
