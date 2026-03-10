mod probe;

use std::path::PathBuf;

use anyhow::{Context, Result};
use common::{
    db::{mark_video_failed, mark_video_processing, mark_video_ready},
    init_tracing, initialize,
    queue::{ack_transcode_job, read_transcode_job, QueuedTranscodeJob},
    storage::get_object,
};
use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
    time::{sleep, Duration},
};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    probe::ensure_probe_available()?;

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

    let temp_path = worker_temp_path(&queued_job.message_id);
    download_source_object(
        services,
        &queued_job.job.source_bucket,
        &queued_job.job.source_key,
        &temp_path,
    )
    .await?;

    match probe::validate_media(&temp_path).await? {
        probe::ProbeOutcome::Valid => {
            info!(
                message_id = %queued_job.message_id,
                video_id = %video.id,
                "worker ffprobe validation passed"
            );
        }
        probe::ProbeOutcome::InvalidMedia { message } => {
            cleanup_temp_file(&temp_path).await;
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

    sleep(Duration::from_secs(1)).await;

    let manifest_key = format!("{}/index.m3u8", queued_job.job.output_prefix);
    let video = mark_video_ready(&services.db, queued_job.job.video_id, &manifest_key).await?;
    cleanup_temp_file(&temp_path).await;
    info!(
        message_id = %queued_job.message_id,
        video_id = %video.id,
        status = %video.status.as_str(),
        hls_manifest_key = %manifest_key,
        "worker finished fake transcode job"
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

fn worker_temp_path(message_id: &str) -> PathBuf {
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

async fn cleanup_temp_file(path: &PathBuf) {
    if let Err(error) = fs::remove_file(path).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            warn!(path = %path.display(), error = %error, "failed to remove worker temp file");
        }
    }
}
