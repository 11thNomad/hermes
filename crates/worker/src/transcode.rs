use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use anyhow::{anyhow, Context, Result};
use tokio::{fs, process::Command};

pub async fn transcode_to_hls(input_path: &Path, output_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(output_dir)
        .await
        .with_context(|| format!("failed to create HLS output dir `{}`", output_dir.display()))?;

    let manifest_path = output_dir.join("index.m3u8");
    let segment_pattern = output_dir.join("segment_%03d.ts");
    let output = Command::new("ffmpeg")
        .arg("-y")
        .arg("-i")
        .arg(input_path)
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg("0:a:0?")
        .arg("-c:v")
        .arg("libx264")
        .arg("-preset")
        .arg("veryfast")
        .arg("-crf")
        .arg("23")
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg("128k")
        .arg("-f")
        .arg("hls")
        .arg("-hls_time")
        .arg("4")
        .arg("-hls_list_size")
        .arg("0")
        .arg("-hls_playlist_type")
        .arg("vod")
        .arg("-hls_flags")
        .arg("independent_segments")
        .arg("-hls_segment_filename")
        .arg(&segment_pattern)
        .arg(&manifest_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .with_context(|| format!("failed to execute ffmpeg for `{}`", input_path.display()))?;

    if output.status.success() {
        return Ok(manifest_path);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = stderr.trim();
    let message = if message.is_empty() {
        "ffmpeg failed to generate HLS output".to_owned()
    } else {
        truncate_error(message, 512)
    };

    Err(anyhow!(message))
}

pub fn ensure_ffmpeg_available() -> Result<()> {
    let status = std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to start ffmpeg for availability check")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "ffmpeg is installed but returned a non-zero exit status"
        ))
    }
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
