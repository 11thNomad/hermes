use anyhow::{Context, Result};
use redis::{Client, RedisError};
use tracing::info;

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
