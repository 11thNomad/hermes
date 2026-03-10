mod probe;
mod transcode;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use common::{
    db::{mark_video_failed, mark_video_processing, mark_video_ready},
    init_tracing, initialize,
    queue::{ack_transcode_job, read_transcode_job, QueuedTranscodeJob},
    storage::{get_object, hls_manifest_key, put_object_path},
};
use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    probe::ensure_probe_available()?;
    transcode::ensure_ffmpeg_available()?;

    let (config, services) = initialize("worker").await?;
    let consumer = consumer_name();
    info!(
        stream = %config.transcode_stream,
        group = %config.transcode_consumer_group,
        consumer = %consumer,
        poll_interval_ms = config.worker_poll_interval_ms,
        "worker bootstrap complete"
    );

    loop {
        match read_transcode_job(
            &services.redis,
            &config.transcode_stream,
            &config.transcode_consumer_group,
            &consumer,
            config.worker_poll_interval_ms as usize,
        )
        .await
        {
            Ok(Some(queued_job)) => {
                if let Err(error) = process_job(&services, &config, queued_job).await {
                    warn!(error = %error, "worker failed to process transcode job");
                }
            }
            Ok(None) => {}
            Err(error) => {
                warn!(error = %error, "worker failed to read transcode job");
            }
        }
    }
}

fn consumer_name() -> String {
    let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "worker".to_owned());
    format!("{hostname}-{}", std::process::id())
}

async fn process_job(
    services: &common::SharedServices,
    config: &common::AppConfig,
    queued_job: QueuedTranscodeJob,
) -> Result<()> {
    let video = mark_video_processing(&services.db, queued_job.job.video_id).await?;
    info!(
        message_id = %queued_job.message_id,
        video_id = %video.id,
        status = %video.status.as_str(),
        attempt_count = video.attempt_count,
        source_key = %queued_job.job.source_key,
        "worker consumed transcode job"
    );

    let job_dir = worker_job_dir(&queued_job.message_id);
    fs::create_dir_all(&job_dir)
        .await
        .map_err(anyhow::Error::from)
        .with_context(|| format!("failed to create worker temp dir `{}`", job_dir.display()))?;
    let source_path = job_dir.join("source");
    download_source_object(
        services,
        &queued_job.job.source_bucket,
        &queued_job.job.source_key,
        &source_path,
    )
    .await?;

    match probe::validate_media(&source_path).await? {
        probe::ProbeOutcome::Valid => {
            info!(
                message_id = %queued_job.message_id,
                video_id = %video.id,
                "worker ffprobe validation passed"
            );
        }
        probe::ProbeOutcome::InvalidMedia { message } => {
            cleanup_temp_dir(&job_dir).await;
            let video = mark_video_failed(&services.db, queued_job.job.video_id, &message).await?;
            warn!(
                message_id = %queued_job.message_id,
                video_id = %video.id,
                status = %video.status.as_str(),
                error = %message,
                "worker rejected invalid media"
            );

            ack_transcode_job(
                &services.redis,
                &config.transcode_stream,
                &config.transcode_consumer_group,
                &queued_job.message_id,
            )
            .await?;

            info!(
                message_id = %queued_job.message_id,
                video_id = %queued_job.job.video_id,
                "worker acknowledged failed transcode job"
            );

            return Ok(());
        }
    }

    let output_dir = job_dir.join("hls");
    if let Err(error) = transcode_and_upload(services, &queued_job, &source_path, &output_dir).await
    {
        cleanup_temp_dir(&job_dir).await;
        let message = truncate_error(&error.to_string(), 512);
        let video = mark_video_failed(&services.db, queued_job.job.video_id, &message).await?;
        warn!(
            message_id = %queued_job.message_id,
            video_id = %video.id,
            status = %video.status.as_str(),
            error = %message,
            "worker HLS generation failed"
        );

        ack_transcode_job(
            &services.redis,
            &config.transcode_stream,
            &config.transcode_consumer_group,
            &queued_job.message_id,
        )
        .await?;

        info!(
            message_id = %queued_job.message_id,
            video_id = %queued_job.job.video_id,
            "worker acknowledged failed transcode job"
        );

        return Ok(());
    }

    let manifest_key = hls_manifest_key(queued_job.job.video_id);
    let video = mark_video_ready(&services.db, queued_job.job.video_id, &manifest_key).await?;
    cleanup_temp_dir(&job_dir).await;
    info!(
        message_id = %queued_job.message_id,
        video_id = %video.id,
        status = %video.status.as_str(),
        hls_manifest_key = %manifest_key,
        "worker finished HLS transcode job"
    );

    ack_transcode_job(
        &services.redis,
        &config.transcode_stream,
        &config.transcode_consumer_group,
        &queued_job.message_id,
    )
    .await?;

    info!(
        message_id = %queued_job.message_id,
        video_id = %queued_job.job.video_id,
        "worker acknowledged transcode job"
    );

    Ok(())
}

fn worker_job_dir(message_id: &str) -> PathBuf {
    let sanitized = message_id.replace(['/', '\\', ':'], "_");
    std::env::temp_dir().join(format!("hermes-worker-{sanitized}"))
}

async fn download_source_object(
    services: &common::SharedServices,
    bucket: &str,
    key: &str,
    destination: &PathBuf,
) -> Result<()> {
    let object = get_object(&services.s3, bucket, key, None).await?;
    let mut file = File::create(destination)
        .await
        .map_err(anyhow::Error::from)
        .with_context(|| format!("failed to create temp file `{}`", destination.display()))?;
    let mut body = object.body;

    while let Some(chunk) = body.try_next().await? {
        file.write_all(&chunk)
            .await
            .map_err(anyhow::Error::from)
            .with_context(|| format!("failed to write temp file `{}`", destination.display()))?;
    }

    file.flush()
        .await
        .map_err(anyhow::Error::from)
        .with_context(|| format!("failed to flush temp file `{}`", destination.display()))?;

    Ok(())
}

async fn cleanup_temp_dir(path: &PathBuf) {
    if let Err(error) = fs::remove_dir_all(path).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            warn!(path = %path.display(), error = %error, "failed to remove worker temp dir");
        }
    }
}

async fn transcode_and_upload(
    services: &common::SharedServices,
    queued_job: &QueuedTranscodeJob,
    source_path: &Path,
    output_dir: &Path,
) -> Result<()> {
    transcode::transcode_to_hls(source_path, output_dir).await?;
    upload_hls_outputs(
        &services.s3,
        &queued_job.job.output_bucket,
        &queued_job.job.output_prefix,
        output_dir,
    )
    .await
}

async fn upload_hls_outputs(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    output_prefix: &str,
    output_dir: &Path,
) -> Result<()> {
    let mut entries = fs::read_dir(output_dir)
        .await
        .map_err(anyhow::Error::from)
        .with_context(|| format!("failed to read HLS output dir `{}`", output_dir.display()))?;
    let prefix = Arc::new(output_prefix.to_owned());
    let bucket = Arc::new(bucket.to_owned());

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(anyhow::Error::from)
        .with_context(|| format!("failed to read HLS output dir `{}`", output_dir.display()))?
    {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .await
            .map_err(anyhow::Error::from)
            .with_context(|| format!("failed to read file type for `{}`", path.display()))?;
        if !file_type.is_file() {
            continue;
        }

        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let object_key = format!("{}/{}", prefix, file_name);
        let content_type = hls_content_type(&path);
        put_object_path(client, &bucket, &object_key, content_type, &path).await?;
        info!(bucket = %bucket, key = %object_key, "worker uploaded HLS asset");
    }

    Ok(())
}

fn hls_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()) {
        Some("m3u8") => "application/vnd.apple.mpegurl",
        Some("ts") => "video/mp2t",
        _ => "application/octet-stream",
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
