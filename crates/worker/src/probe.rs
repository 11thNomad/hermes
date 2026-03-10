use std::{path::Path, process::Stdio};

use anyhow::{anyhow, Context, Result};
use tokio::process::Command;

#[derive(Debug)]
pub enum ProbeOutcome {
    Valid,
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
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .with_context(|| format!("failed to execute ffprobe for `{}`", path.display()))?;

    if output.status.success() {
        return Ok(ProbeOutcome::Valid);
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
    use super::truncate_error;

    #[test]
    fn truncate_error_preserves_short_messages() {
        assert_eq!(truncate_error("short", 10), "short");
    }

    #[test]
    fn truncate_error_adds_ellipsis_for_long_messages() {
        let truncated = truncate_error("abcdefghijklmnopqrstuvwxyz", 10);
        assert_eq!(truncated, "abcdefg...");
    }
}
