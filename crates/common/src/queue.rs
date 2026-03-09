use anyhow::{Context, Result};
use redis::{
    streams::{StreamReadOptions, StreamReadReply},
    Client, RedisError,
};
use tracing::info;

use crate::models::TranscodeJob;

pub async fn ensure_consumer_group(client: &Client, stream: &str, group: &str) -> Result<()> {
    let mut connection = client
        .get_multiplexed_async_connection()
        .await
        .context("failed to open Redis connection for bootstrap")?;

    let result = redis::cmd("XGROUP")
        .arg("CREATE")
        .arg(stream)
        .arg(group)
        .arg("$")
        .arg("MKSTREAM")
        .query_async::<String>(&mut connection)
        .await;

    match result {
        Ok(_) => {
            info!(stream, group, "redis consumer group created");
            Ok(())
        }
        Err(error) if is_busy_group_error(&error) => {
            info!(stream, group, "redis consumer group already exists");
            Ok(())
        }
        Err(error) => Err(error).context("failed to create Redis consumer group"),
    }
}

fn is_busy_group_error(error: &RedisError) -> bool {
    error.code() == Some("BUSYGROUP")
}

pub async fn enqueue_transcode_job(
    client: &Client,
    stream: &str,
    job: &TranscodeJob,
) -> Result<String> {
    let payload = serde_json::to_string(job).context("failed to serialize transcode job")?;
    let mut connection = client
        .get_multiplexed_async_connection()
        .await
        .context("failed to open Redis connection for enqueue")?;

    redis::cmd("XADD")
        .arg(stream)
        .arg("*")
        .arg("payload")
        .arg(payload)
        .query_async::<String>(&mut connection)
        .await
        .with_context(|| format!("failed to enqueue transcode job on stream `{stream}`"))
}

#[derive(Debug, Clone)]
pub struct QueuedTranscodeJob {
    pub message_id: String,
    pub job: TranscodeJob,
}

pub async fn read_transcode_job(
    client: &Client,
    stream: &str,
    group: &str,
    consumer: &str,
    block_ms: usize,
) -> Result<Option<QueuedTranscodeJob>> {
    let mut connection = client
        .get_multiplexed_async_connection()
        .await
        .context("failed to open Redis connection for stream read")?;

    let options = StreamReadOptions::default()
        .group(group, consumer)
        .count(1)
        .block(block_ms);
    let reply = redis::cmd("XREADGROUP")
        .arg(&options)
        .arg("STREAMS")
        .arg(stream)
        .arg(">")
        .query_async::<Option<StreamReadReply>>(&mut connection)
        .await
        .with_context(|| {
            format!("failed to read transcode job from stream `{stream}` for consumer `{consumer}`")
        })?;

    let Some(reply) = reply else {
        return Ok(None);
    };

    let Some(message) = reply
        .keys
        .into_iter()
        .find(|key| key.key == stream)
        .and_then(|key| key.ids.into_iter().next())
    else {
        return Ok(None);
    };

    let payload = message
        .get::<String>("payload")
        .context("stream message is missing `payload` field")?;
    let job = serde_json::from_str::<TranscodeJob>(&payload)
        .context("failed to deserialize transcode job payload")?;

    Ok(Some(QueuedTranscodeJob {
        message_id: message.id,
        job,
    }))
}

pub async fn ack_transcode_job(
    client: &Client,
    stream: &str,
    group: &str,
    message_id: &str,
) -> Result<()> {
    let mut connection = client
        .get_multiplexed_async_connection()
        .await
        .context("failed to open Redis connection for stream ack")?;

    let acked = redis::cmd("XACK")
        .arg(stream)
        .arg(group)
        .arg(message_id)
        .query_async::<usize>(&mut connection)
        .await
        .with_context(|| format!("failed to ack transcode job `{message_id}`"))?;

    if acked == 0 {
        anyhow::bail!("redis did not ack transcode job `{message_id}`");
    }

    Ok(())
}
