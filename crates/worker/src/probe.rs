use std::{path::Path, process::Stdio, time::Duration};

use anyhow::{anyhow, Context, Result};
use tokio::process::Command;

#[derive(Debug, Clone, Copy)]
pub struct MediaMetadata {
    pub duration: Option<Duration>,
}

#[derive(Debug)]
pub enum ProbeOutcome {
    Valid(MediaMetadata),
    InvalidMedia { message: String },
}

pub async fn validate_media(path: &Path) -> Result<ProbeOutcome> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format=duration")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .with_context(|| format!("failed to execute ffprobe for `{}`", path.display()))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let duration = parse_duration(stdout.trim());
        return Ok(ProbeOutcome::Valid(MediaMetadata { duration }));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = stderr.trim();
    let message = if message.is_empty() {
        "ffprobe rejected the media".to_owned()
    } else {
        truncate_error(message, 512)
    };

    Ok(ProbeOutcome::InvalidMedia { message })
}

fn truncate_error(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_owned();
    }

    let mut truncated = value
        .chars()
        .take(max_len.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

fn parse_duration(value: &str) -> Option<Duration> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("n/a") {
        return None;
    }

    trimmed.parse::<f64>().ok().and_then(|seconds| {
        if seconds.is_sign_negative() || !seconds.is_finite() {
            None
        } else {
            Some(Duration::from_secs_f64(seconds))
        }
    })
}

pub fn ensure_probe_available() -> Result<()> {
    let status = std::process::Command::new("ffprobe")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to start ffprobe for availability check")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "ffprobe is installed but returned a non-zero exit status"
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{parse_duration, truncate_error};

    #[test]
    fn truncate_error_preserves_short_messages() {
        assert_eq!(truncate_error("short", 10), "short");
    }

    #[test]
    fn truncate_error_adds_ellipsis_for_long_messages() {
        let truncated = truncate_error("abcdefghijklmnopqrstuvwxyz", 10);
        assert_eq!(truncated, "abcdefg...");
    }

    #[test]
    fn parse_duration_reads_decimal_seconds() {
        assert_eq!(
            parse_duration("123.456"),
            Some(Duration::from_secs_f64(123.456))
        );
    }

    #[test]
    fn parse_duration_rejects_missing_values() {
        assert_eq!(parse_duration("N/A"), None);
        assert_eq!(parse_duration(""), None);
    }
}
