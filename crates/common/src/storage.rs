use anyhow::{Context, Result};
use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_s3::{error::SdkError, operation::head_bucket::HeadBucketError, Client};
use tracing::info;

use crate::config::AppConfig;

pub async fn build_s3_client(config: &AppConfig) -> Result<Client> {
    let shared_config = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(config.s3_region.clone()))
        .credentials_provider(Credentials::new(
            config.s3_access_key.clone(),
            config.s3_secret_key.clone(),
            None,
            None,
            "hermes-bootstrap",
        ))
        .endpoint_url(config.s3_endpoint.clone())
        .load()
        .await;

    let s3_config = aws_sdk_s3::config::Builder::from(&shared_config)
        .force_path_style(true)
        .build();

    Ok(Client::from_conf(s3_config))
}

pub async fn ensure_bucket(client: &Client, bucket: &str) -> Result<()> {
    match client.head_bucket().bucket(bucket).send().await {
        Ok(_) => {
            info!(bucket, "object storage bucket already exists");
            return Ok(());
        }
        Err(SdkError::ServiceError(error))
            if matches!(error.err(), HeadBucketError::NotFound(_)) => {}
        Err(error) => {
            return Err(error).with_context(|| format!("failed while checking bucket `{bucket}`"));
        }
    }

    info!(bucket, "creating object storage bucket");
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .with_context(|| format!("failed to create bucket `{bucket}`"))?;
    info!(bucket, "object storage bucket created");

    Ok(())
}
