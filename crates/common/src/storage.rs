use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_s3::{
    error::SdkError,
    operation::{
        create_bucket::CreateBucketError, get_object::GetObjectOutput, head_bucket::HeadBucketError,
    },
    presigning::PresigningConfig,
    primitives::ByteStream,
    Client,
};
use tracing::info;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::models::DetectedFormat;

pub async fn build_s3_client(config: &AppConfig) -> Result<Client> {
    build_s3_client_for_endpoint(config, &config.s3_endpoint).await
}

pub async fn build_public_s3_client(config: &AppConfig) -> Result<Client> {
    build_s3_client_for_endpoint(config, &config.s3_public_endpoint).await
}

async fn build_s3_client_for_endpoint(config: &AppConfig, endpoint: &str) -> Result<Client> {
    let shared_config = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(config.s3_region.clone()))
        .credentials_provider(Credentials::new(
            config.s3_access_key.clone(),
            config.s3_secret_key.clone(),
            None,
            None,
            "hermes-bootstrap",
        ))
        .endpoint_url(endpoint)
        .load()
        .await;

    let s3_config = aws_sdk_s3::config::Builder::from(&shared_config)
        .force_path_style(true)
        .request_checksum_calculation(aws_sdk_s3::config::RequestChecksumCalculation::WhenRequired)
        .response_checksum_validation(aws_sdk_s3::config::ResponseChecksumValidation::WhenRequired)
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
    match client.create_bucket().bucket(bucket).send().await {
        Ok(_) => {}
        Err(SdkError::ServiceError(e))
            if matches!(
                e.err(),
                CreateBucketError::BucketAlreadyExists(_)
                    | CreateBucketError::BucketAlreadyOwnedByYou(_)
            ) =>
        {
            info!(bucket, "object storage bucket already exists");
            return Ok(());
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to create bucket `{bucket}`"));
        }
    }
    info!(bucket, "object storage bucket created");

    Ok(())
}

pub fn raw_object_key(video_id: Uuid, format: DetectedFormat) -> String {
    format!("{video_id}/original.{}", format.extension())
}

pub fn incoming_object_key(video_id: Uuid) -> String {
    format!("incoming/{video_id}/upload.bin")
}

pub fn hls_manifest_key(video_id: Uuid) -> String {
    format!("{video_id}/hls/index.m3u8")
}

pub fn hls_output_prefix(video_id: Uuid) -> String {
    format!("{video_id}/hls")
}

pub async fn put_object_stream(
    client: &Client,
    bucket: &str,
    key: &str,
    content_type: &str,
    body: ByteStream,
) -> Result<()> {
    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .content_type(content_type)
        .body(body)
        .send()
        .await
        .with_context(|| format!("failed to upload object `{bucket}/{key}`"))?;

    Ok(())
}

pub async fn presign_put_object(
    client: &Client,
    bucket: &str,
    key: &str,
    expires_in: Duration,
) -> Result<String> {
    let presigned = client
        .put_object()
        .bucket(bucket)
        .key(key)
        .presigned(
            PresigningConfig::expires_in(expires_in)
                .context("invalid presign expiration for upload URL")?,
        )
        .await
        .with_context(|| format!("failed to presign upload URL for `{bucket}/{key}`"))?;

    Ok(presigned.uri().to_string())
}

pub async fn put_object_path(
    client: &Client,
    bucket: &str,
    key: &str,
    content_type: &str,
    path: &Path,
) -> Result<()> {
    let body = ByteStream::from_path(path)
        .await
        .with_context(|| format!("failed to open object source `{}`", path.display()))?;

    put_object_stream(client, bucket, key, content_type, body).await
}

pub async fn get_object(
    client: &Client,
    bucket: &str,
    key: &str,
    range: Option<&str>,
) -> Result<GetObjectOutput> {
    let mut request = client.get_object().bucket(bucket).key(key);
    if let Some(range) = range {
        request = request.range(range);
    }

    request
        .send()
        .await
        .with_context(|| format!("failed to fetch object `{bucket}/{key}`"))
}

pub async fn get_object_bytes(
    client: &Client,
    bucket: &str,
    key: &str,
    range: Option<&str>,
) -> Result<Vec<u8>> {
    let object = get_object(client, bucket, key, range).await?;
    let bytes = object
        .body
        .collect()
        .await
        .with_context(|| format!("failed to read object body `{bucket}/{key}`"))?;

    Ok(bytes.into_bytes().to_vec())
}

pub async fn head_object(
    client: &Client,
    bucket: &str,
    key: &str,
) -> Result<aws_sdk_s3::operation::head_object::HeadObjectOutput> {
    client
        .head_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .with_context(|| format!("failed to inspect object `{bucket}/{key}`"))
}

pub async fn copy_object(
    client: &Client,
    bucket: &str,
    source_key: &str,
    destination_key: &str,
    content_type: &str,
) -> Result<()> {
    client
        .copy_object()
        .bucket(bucket)
        .key(destination_key)
        .copy_source(format!("{bucket}/{source_key}"))
        .metadata_directive(aws_sdk_s3::types::MetadataDirective::Replace)
        .content_type(content_type)
        .send()
        .await
        .with_context(|| {
            format!("failed to copy object `{bucket}/{source_key}` to `{bucket}/{destination_key}`")
        })?;

    Ok(())
}

pub async fn delete_object(client: &Client, bucket: &str, key: &str) -> Result<()> {
    client
        .delete_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .with_context(|| format!("failed to delete object `{bucket}/{key}`"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::incoming_object_key;
    use uuid::Uuid;

    #[test]
    fn incoming_object_key_uses_isolated_prefix() {
        let key = incoming_object_key(Uuid::nil());
        assert_eq!(
            key,
            "incoming/00000000-0000-0000-0000-000000000000/upload.bin"
        );
    }
}
