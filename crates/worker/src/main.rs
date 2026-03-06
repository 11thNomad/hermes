use std::time::Duration;

use anyhow::Result;
use common::{init_tracing, initialize};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let (config, _services) = initialize("worker").await?;
    info!(
        stream = %config.transcode_stream,
        group = %config.transcode_consumer_group,
        poll_interval_ms = config.worker_poll_interval_ms,
        "worker bootstrap complete"
    );

    let interval = Duration::from_millis(config.worker_poll_interval_ms);
    loop {
        info!("worker heartbeat");
        tokio::time::sleep(interval).await;
    }
}
