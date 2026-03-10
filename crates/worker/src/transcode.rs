use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use tokio::{
    fs,
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};
use tracing::info;
use uuid::Uuid;

pub struct TranscodeProgressContext<'a> {
    pub message_id: &'a str,
    pub video_id: Uuid,
    pub input_duration: Option<Duration>,
}

pub async fn transcode_to_hls(
    input_path: &Path,
    output_dir: &Path,
    progress_context: TranscodeProgressContext<'_>,
) -> Result<PathBuf> {
    fs::create_dir_all(output_dir)
        .await
        .with_context(|| format!("failed to create HLS output dir `{}`", output_dir.display()))?;

    let manifest_path = output_dir.join("index.m3u8");
    let segment_pattern = output_dir.join("segment_%03d.ts");
    let mut child = Command::new("ffmpeg")
        .arg("-y")
        .arg("-nostats")
        .arg("-loglevel")
        .arg("error")
        .arg("-progress")
        .arg("pipe:2")
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
        .spawn()
        .with_context(|| format!("failed to execute ffmpeg for `{}`", input_path.display()))?;

    let stderr = child
        .stderr
        .take()
        .context("failed to capture ffmpeg progress stream")?;
    let mut progress_reader = BufReader::new(stderr).lines();
    let mut progress_snapshot = FfmpegProgress::default();
    let mut progress_tracker = ProgressTracker::new(progress_context.input_duration);
    let mut error_lines = Vec::new();

    while let Some(line) = progress_reader.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some((key, value)) = trimmed.split_once('=') {
            match key {
                "frame" => progress_snapshot.frame = Some(value.to_owned()),
                "fps" => progress_snapshot.fps = Some(value.to_owned()),
                "speed" => progress_snapshot.speed = Some(value.to_owned()),
                "out_time" => progress_snapshot.out_time = parse_ffmpeg_timestamp(value),
                "progress" => {
                    progress_tracker.log_update(
                        progress_context.message_id,
                        progress_context.video_id,
                        &progress_snapshot,
                        value,
                    );
                    progress_snapshot = FfmpegProgress::default();
                }
                _ => {}
            }
            continue;
        }

        error_lines.push(trimmed.to_owned());
    }

    let status = child
        .wait()
        .await
        .with_context(|| format!("failed to execute ffmpeg for `{}`", input_path.display()))?;

    if status.success() {
        return Ok(manifest_path);
    }

    let message = error_lines.join("\n");
    let message = if message.is_empty() {
        "ffmpeg failed to generate HLS output".to_owned()
    } else {
        truncate_error(&message, 512)
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

#[derive(Default)]
struct FfmpegProgress {
    frame: Option<String>,
    fps: Option<String>,
    speed: Option<String>,
    out_time: Option<Duration>,
}

struct ProgressTracker {
    input_duration: Option<Duration>,
    last_percent_bucket: Option<u64>,
    last_logged_out_time: Option<Duration>,
}

impl ProgressTracker {
    fn new(input_duration: Option<Duration>) -> Self {
        Self {
            input_duration,
            last_percent_bucket: None,
            last_logged_out_time: None,
        }
    }

    fn log_update(
        &mut self,
        message_id: &str,
        video_id: Uuid,
        snapshot: &FfmpegProgress,
        progress_marker: &str,
    ) {
        let should_log = if progress_marker == "end" {
            true
        } else {
            self.should_log_progress(snapshot.out_time)
        };

        if !should_log {
            return;
        }

        let processed_secs = snapshot
            .out_time
            .map(|value| round_tenths(value.as_secs_f64()));
        let total_secs = self
            .input_duration
            .map(|value| round_tenths(value.as_secs_f64()));
        let percent_complete = match (snapshot.out_time, self.input_duration) {
            (Some(current), Some(total)) if !total.is_zero() => Some(round_tenths(
                (current.as_secs_f64() / total.as_secs_f64() * 100.0).clamp(0.0, 100.0),
            )),
            _ => None,
        };

        info!(
            message_id = message_id,
            video_id = %video_id,
            progress = progress_marker,
            percent_complete = ?percent_complete,
            processed_secs = ?processed_secs,
            total_secs = ?total_secs,
            frame = snapshot.frame.as_deref().unwrap_or("unknown"),
            fps = snapshot.fps.as_deref().unwrap_or("unknown"),
            speed = snapshot.speed.as_deref().unwrap_or("unknown"),
            "worker ffmpeg progress"
        );
    }

    fn should_log_progress(&mut self, out_time: Option<Duration>) -> bool {
        let Some(current) = out_time else {
            return false;
        };

        if let Some(total) = self.input_duration {
            if total.is_zero() {
                return false;
            }

            let percent = (current.as_secs_f64() / total.as_secs_f64() * 100.0).clamp(0.0, 100.0);
            let bucket = (percent / 5.0).floor() as u64;
            let should_log = match self.last_percent_bucket {
                Some(last_bucket) => bucket > last_bucket,
                None => true,
            };
            if should_log {
                self.last_percent_bucket = Some(bucket);
            }
            return should_log;
        }

        let should_log = match self.last_logged_out_time {
            Some(last) => current >= last + Duration::from_secs(15),
            None => true,
        };
        if should_log {
            self.last_logged_out_time = Some(current);
        }
        should_log
    }
}

fn parse_ffmpeg_timestamp(value: &str) -> Option<Duration> {
    let mut segments = value.trim().split(':');
    let hours = segments.next()?.parse::<u64>().ok()?;
    let minutes = segments.next()?.parse::<u64>().ok()?;
    let seconds = segments.next()?;
    if segments.next().is_some() {
        return None;
    }

    let (whole_seconds, fractional_seconds) = match seconds.split_once('.') {
        Some((whole_seconds, fractional_seconds)) => (whole_seconds, fractional_seconds),
        None => (seconds, ""),
    };

    let whole_seconds = whole_seconds.parse::<u64>().ok()?;
    let mut nanos = fractional_seconds
        .chars()
        .filter(char::is_ascii_digit)
        .take(9)
        .collect::<String>();
    while nanos.len() < 9 {
        nanos.push('0');
    }
    let nanos = if nanos.is_empty() {
        0
    } else {
        nanos.parse::<u32>().ok()?
    };

    let total_seconds = hours
        .checked_mul(3600)?
        .checked_add(minutes.checked_mul(60)?)?
        .checked_add(whole_seconds)?;
    Some(Duration::from_secs(total_seconds) + Duration::from_nanos(u64::from(nanos)))
}

fn round_tenths(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{parse_ffmpeg_timestamp, truncate_error};

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
    fn parse_ffmpeg_timestamp_reads_fractional_values() {
        assert_eq!(
            parse_ffmpeg_timestamp("00:02:03.456789"),
            Some(Duration::from_secs_f64(123.456789))
        );
    }

    #[test]
    fn parse_ffmpeg_timestamp_rejects_bad_values() {
        assert_eq!(parse_ffmpeg_timestamp("not-a-timestamp"), None);
    }
}
