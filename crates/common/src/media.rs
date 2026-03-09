use crate::models::DetectedFormat;

pub const MAX_SNIFF_BYTES: usize = 64;

pub fn detect_format(bytes: &[u8]) -> Option<DetectedFormat> {
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        let brand = &bytes[8..12];
        if matches!(brand, b"qt  " | b"moov") {
            return Some(DetectedFormat::Mov);
        }

        return Some(DetectedFormat::Mp4);
    }

    if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        if bytes.windows(4).any(|window| window == b"webm") {
            return Some(DetectedFormat::Webm);
        }

        return Some(DetectedFormat::Mkv);
    }

    None
}

pub fn sanitize_filename(filename: Option<&str>, format: DetectedFormat) -> String {
    let candidate = filename
        .and_then(|value| value.rsplit('/').next())
        .and_then(|value| value.rsplit('\\').next())
        .unwrap_or("upload");

    let sanitized = candidate
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_owned();

    let fallback = format!("upload.{}", format.extension());
    if sanitized.is_empty() {
        return fallback;
    }

    if sanitized.rsplit('.').next() == Some(format.extension()) {
        sanitized
    } else {
        format!("{sanitized}.{}", format.extension())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_mp4_and_mov() {
        let mp4 = b"\x00\x00\x00\x18ftypisom\x00\x00\x02\x00";
        let mov = b"\x00\x00\x00\x14ftypqt  \x00\x00\x00\x00";

        assert_eq!(detect_format(mp4), Some(DetectedFormat::Mp4));
        assert_eq!(detect_format(mov), Some(DetectedFormat::Mov));
    }

    #[test]
    fn detects_matroska_and_webm() {
        let mkv = b"\x1A\x45\xDF\xA3\x93B\x82\x88matroska";
        let webm = b"\x1A\x45\xDF\xA3\x93B\x82\x84webm";

        assert_eq!(detect_format(mkv), Some(DetectedFormat::Mkv));
        assert_eq!(detect_format(webm), Some(DetectedFormat::Webm));
    }

    #[test]
    fn sanitizes_and_appends_extension() {
        assert_eq!(
            sanitize_filename(Some("../unsafe name"), DetectedFormat::Mp4),
            "unsafe_name.mp4"
        );
        assert_eq!(
            sanitize_filename(Some("clip.webm"), DetectedFormat::Webm),
            "clip.webm"
        );
    }
}
