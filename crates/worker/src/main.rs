use anyhow::Result;
use common::{
    db::{mark_video_processing, mark_video_ready},
    init_tracing, initialize,
    queue::{ack_transcode_job, read_transcode_job, QueuedTranscodeJob},
};
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

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

    sleep(Duration::from_secs(1)).await;

    let manifest_key = format!("{}/index.m3u8", queued_job.job.output_prefix);
    let video = mark_video_ready(&services.db, queued_job.job.video_id, &manifest_key).await?;
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
