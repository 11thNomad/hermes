use std::{path::PathBuf, time::Duration};

use aws_sdk_s3::primitives::ByteStream;
use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{header::CONTENT_LENGTH, HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use common::{
    db::{delete_video, insert_video, list_recent_videos, NewVideo},
    media::{detect_format, sanitize_filename, MAX_SNIFF_BYTES},
    models::{TranscodeJob, UploadVideoResponse, VideoListItem, VideoStatus},
    queue::enqueue_transcode_job,
    storage::{
        copy_object, delete_object, get_object_bytes, head_object, hls_output_prefix,
        incoming_object_key, presign_put_object, put_object_stream, raw_object_key,
    },
};
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
};
use tracing::{info, warn};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{routes::ApiError, state::AppState};

const MULTIPART_OVERHEAD_GRACE_BYTES: u64 = 64 * 1024;
const DIRECT_UPLOAD_URL_TTL_SECS: u64 = 15 * 60;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/videos", get(list_videos).post(upload_video))
        .route("/api/uploads/init", post(init_direct_upload))
        .route("/api/uploads/:id/complete", post(complete_direct_upload))
        .layer(DefaultBodyLimit::disable())
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct DirectUploadInitRequest {
    pub(crate) filename: String,
    pub(crate) size_bytes: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct DirectUploadInitResponse {
    pub(crate) video_id: Uuid,
    pub(crate) upload_url: String,
    pub(crate) shareable_url: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct DirectUploadCompleteRequest {
    pub(crate) filename: String,
}

#[utoipa::path(
    get,
    path = "/api/videos",
    tag = "videos",
    responses(
        (status = 200, description = "Recent uploaded videos", body = [VideoListItem]),
        (status = 500, description = "Internal server error", body = crate::routes::ErrorResponse)
    )
)]
pub(crate) async fn list_videos(
    State(state): State<AppState>,
) -> Result<Json<Vec<VideoListItem>>, ApiError> {
    let videos = list_recent_videos(&state.services.db, 25).await?;
    Ok(Json(videos))
}

#[utoipa::path(
    post,
    path = "/api/uploads/init",
    tag = "videos",
    request_body = DirectUploadInitRequest,
    responses(
        (status = 200, description = "Direct upload session initialized", body = DirectUploadInitResponse),
        (status = 400, description = "Invalid upload request", body = crate::routes::ErrorResponse),
        (status = 413, description = "Upload too large", body = crate::routes::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::routes::ErrorResponse)
    )
)]
pub(crate) async fn init_direct_upload(
    State(state): State<AppState>,
    Json(payload): Json<DirectUploadInitRequest>,
) -> Result<Json<DirectUploadInitResponse>, ApiError> {
    if payload.size_bytes == 0 {
        return Err(ApiError::BadRequest("video file was empty".to_owned()));
    }

    if payload.size_bytes > state.config.max_upload_bytes {
        return Err(ApiError::PayloadTooLarge(format!(
            "upload exceeds {} bytes",
            state.config.max_upload_bytes
        )));
    }

    let video_id = Uuid::new_v4();
    let temp_key = incoming_object_key(video_id);
    let upload_url = presign_put_object(
        &state.services.s3_public,
        &state.config.raw_bucket,
        &temp_key,
        Duration::from_secs(DIRECT_UPLOAD_URL_TTL_SECS),
    )
    .await
    .map_err(ApiError::Internal)?;

    info!(
        video_id = %video_id,
        filename = %payload.filename,
        declared_size_bytes = payload.size_bytes,
        temp_key = %temp_key,
        "direct upload initialized"
    );

    Ok(Json(DirectUploadInitResponse {
        video_id,
        upload_url,
        shareable_url: shareable_url(video_id),
    }))
}

#[utoipa::path(
    post,
    path = "/api/uploads/{id}/complete",
    tag = "videos",
    params(
        ("id" = Uuid, Path, description = "Video identifier")
    ),
    request_body = DirectUploadCompleteRequest,
    responses(
        (status = 201, description = "Upload finalized and queued", body = UploadVideoResponse),
        (status = 400, description = "Invalid upload request", body = crate::routes::ErrorResponse),
        (status = 413, description = "Upload too large", body = crate::routes::ErrorResponse),
        (status = 415, description = "Unsupported video format", body = crate::routes::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::routes::ErrorResponse)
    )
)]
pub(crate) async fn complete_direct_upload(
    State(state): State<AppState>,
    Path(video_id): Path<Uuid>,
    Json(payload): Json<DirectUploadCompleteRequest>,
) -> Result<(StatusCode, Json<UploadVideoResponse>), ApiError> {
    let temp_key = incoming_object_key(video_id);
    let object = head_object(&state.services.s3, &state.config.raw_bucket, &temp_key)
        .await
        .map_err(ApiError::Internal)?;
    let size_bytes = object.content_length().unwrap_or_default().max(0) as u64;

    if size_bytes == 0 {
        cleanup_object(&state, &temp_key).await;
        return Err(ApiError::BadRequest("uploaded object was empty".to_owned()));
    }

    if size_bytes > state.config.max_upload_bytes {
        cleanup_object(&state, &temp_key).await;
        return Err(ApiError::PayloadTooLarge(format!(
            "upload exceeds {} bytes",
            state.config.max_upload_bytes
        )));
    }

    let sniff_range = format!("bytes=0-{}", MAX_SNIFF_BYTES.saturating_sub(1));
    let sniffed = get_object_bytes(
        &state.services.s3,
        &state.config.raw_bucket,
        &temp_key,
        Some(&sniff_range),
    )
    .await
    .map_err(ApiError::Internal)?;

    let Some(detected_format) = detect_format(&sniffed) else {
        cleanup_object(&state, &temp_key).await;
        return Err(ApiError::UnsupportedMediaType(
            "unsupported video format".to_owned(),
        ));
    };

    let filename = sanitize_filename(Some(payload.filename.as_str()), detected_format);
    let raw_key = raw_object_key(video_id, detected_format);
    copy_object(
        &state.services.s3,
        &state.config.raw_bucket,
        &temp_key,
        &raw_key,
        detected_format.mime_type(),
    )
    .await
    .map_err(ApiError::Internal)?;

    let insert_result = insert_video(
        &state.services.db,
        &NewVideo {
            id: video_id,
            filename,
            mime_type: detected_format.mime_type().to_owned(),
            size_bytes: size_bytes as i64,
            status: VideoStatus::Pending,
            raw_key: raw_key.clone(),
            detected_format,
        },
    )
    .await;
    let record = match insert_result {
        Ok(record) => record,
        Err(error) => {
            cleanup_object(&state, &raw_key).await;
            cleanup_object(&state, &temp_key).await;
            return Err(ApiError::Internal(error));
        }
    };

    let job = TranscodeJob {
        video_id,
        source_bucket: state.config.raw_bucket.clone(),
        source_key: raw_key.clone(),
        output_bucket: state.config.hls_bucket.clone(),
        output_prefix: hls_output_prefix(video_id),
    };

    if let Err(error) =
        enqueue_transcode_job(&state.services.redis, &state.config.transcode_stream, &job).await
    {
        warn!(
            video_id = %video_id,
            error = %error,
            "failed to enqueue direct upload transcode job, cleaning up upload"
        );
        if let Err(cleanup_error) = delete_video(&state.services.db, video_id).await {
            warn!(video_id = %video_id, error = %cleanup_error, "failed to rollback video row");
        }
        cleanup_object(&state, &raw_key).await;
        cleanup_object(&state, &temp_key).await;
        return Err(ApiError::Internal(error));
    }

    cleanup_object(&state, &temp_key).await;
    info!(
        video_id = %record.id,
        size_bytes,
        raw_key = %record.raw_key,
        "direct upload finalized"
    );
    Ok((
        StatusCode::CREATED,
        Json(UploadVideoResponse {
            id: record.id,
            shareable_url: shareable_url(record.id),
        }),
    ))
}

#[utoipa::path(
    post,
    path = "/api/videos",
    tag = "videos",
    request_body(
        content = inline(crate::docs::UploadVideoRequest),
        content_type = "multipart/form-data",
        description = "Legacy multipart upload with a single `video` file field"
    ),
    responses(
        (status = 201, description = "Upload accepted", body = UploadVideoResponse),
        (status = 400, description = "Invalid multipart payload", body = crate::routes::ErrorResponse),
        (status = 413, description = "Upload too large", body = crate::routes::ErrorResponse),
        (status = 415, description = "Unsupported video format", body = crate::routes::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::routes::ErrorResponse)
    )
)]
pub(crate) async fn upload_video(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<UploadVideoResponse>), ApiError> {
    if header_upload_too_large(
        &headers,
        state.config.max_upload_bytes,
        MULTIPART_OVERHEAD_GRACE_BYTES,
    )? {
        return Err(ApiError::PayloadTooLarge(format!(
            "upload exceeds {} bytes",
            state.config.max_upload_bytes
        )));
    }

    let mut field =
        loop {
            match multipart.next_field().await.map_err(|error| {
                ApiError::BadRequest(format!("invalid multipart upload: {error}"))
            })? {
                Some(field) if field.name() == Some("video") => break field,
                Some(_) => continue,
                None => {
                    return Err(ApiError::BadRequest(
                        "multipart field `video` is required".to_owned(),
                    ));
                }
            }
        };
    let original_filename = field.file_name().map(ToOwned::to_owned);
    let temp_path = upload_temp_path();
    let mut temp_file = create_temp_file(&temp_path).await?;
    let mut sniffed = Vec::with_capacity(MAX_SNIFF_BYTES);
    let mut size_bytes = 0u64;

    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|error| ApiError::BadRequest(format!("failed to read upload body: {error}")))?
    {
        size_bytes = size_bytes
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| ApiError::PayloadTooLarge("upload size overflowed".to_owned()))?;
        if size_bytes > state.config.max_upload_bytes {
            cleanup_temp_file(&temp_path).await;
            return Err(ApiError::PayloadTooLarge(format!(
                "upload exceeds {} bytes",
                state.config.max_upload_bytes
            )));
        }

        if sniffed.len() < MAX_SNIFF_BYTES {
            let remaining = MAX_SNIFF_BYTES - sniffed.len();
            sniffed.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        }

        temp_file
            .write_all(&chunk)
            .await
            .map_err(|error| ApiError::Internal(error.into()))?;
    }

    temp_file
        .flush()
        .await
        .map_err(|error| ApiError::Internal(error.into()))?;
    drop(temp_file);

    if size_bytes == 0 {
        cleanup_temp_file(&temp_path).await;
        return Err(ApiError::BadRequest("video field was empty".to_owned()));
    }

    let Some(detected_format) = detect_format(&sniffed) else {
        cleanup_temp_file(&temp_path).await;
        return Err(ApiError::UnsupportedMediaType(
            "unsupported video format".to_owned(),
        ));
    };
    let filename = sanitize_filename(original_filename.as_deref(), detected_format);
    let video_id = Uuid::new_v4();
    let raw_key = raw_object_key(video_id, detected_format);

    let body = ByteStream::from_path(&temp_path)
        .await
        .map_err(|error| ApiError::Internal(anyhow::Error::from(error)))?;
    let upload_result = put_object_stream(
        &state.services.s3,
        &state.config.raw_bucket,
        &raw_key,
        detected_format.mime_type(),
        body,
    )
    .await;
    cleanup_temp_file(&temp_path).await;
    upload_result?;

    let insert_result = insert_video(
        &state.services.db,
        &NewVideo {
            id: video_id,
            filename,
            mime_type: detected_format.mime_type().to_owned(),
            size_bytes: size_bytes as i64,
            status: VideoStatus::Pending,
            raw_key: raw_key.clone(),
            detected_format,
        },
    )
    .await;
    let record = match insert_result {
        Ok(record) => record,
        Err(error) => {
            if let Err(cleanup_error) =
                delete_object(&state.services.s3, &state.config.raw_bucket, &raw_key).await
            {
                warn!(
                    video_id = %video_id,
                    error = %cleanup_error,
                    "failed to cleanup raw object after db insert error"
                );
            }
            return Err(ApiError::Internal(error));
        }
    };

    let job = TranscodeJob {
        video_id,
        source_bucket: state.config.raw_bucket.clone(),
        source_key: raw_key.clone(),
        output_bucket: state.config.hls_bucket.clone(),
        output_prefix: hls_output_prefix(video_id),
    };

    if let Err(error) =
        enqueue_transcode_job(&state.services.redis, &state.config.transcode_stream, &job).await
    {
        warn!(video_id = %video_id, error = %error, "failed to enqueue transcode job, cleaning up upload");
        if let Err(cleanup_error) = delete_video(&state.services.db, video_id).await {
            warn!(video_id = %video_id, error = %cleanup_error, "failed to rollback video row");
        }
        if let Err(cleanup_error) =
            delete_object(&state.services.s3, &state.config.raw_bucket, &raw_key).await
        {
            warn!(video_id = %video_id, error = %cleanup_error, "failed to rollback raw object");
        }
        return Err(ApiError::Internal(error));
    }

    info!(video_id = %record.id, size_bytes, raw_key = %record.raw_key, "video uploaded");
    Ok((
        StatusCode::CREATED,
        Json(UploadVideoResponse {
            id: record.id,
            shareable_url: shareable_url(record.id),
        }),
    ))
}

fn header_upload_too_large(
    headers: &HeaderMap,
    max_upload_bytes: u64,
    overhead_grace_bytes: u64,
) -> Result<bool, ApiError> {
    let Some(raw_value) = headers.get(CONTENT_LENGTH) else {
        return Ok(false);
    };

    let raw_value = raw_value
        .to_str()
        .map_err(|_| ApiError::BadRequest("invalid Content-Length header".to_owned()))?;
    let parsed = raw_value
        .parse::<u64>()
        .map_err(|_| ApiError::BadRequest("invalid Content-Length header".to_owned()))?;

    Ok(parsed > max_upload_bytes.saturating_add(overhead_grace_bytes))
}

fn shareable_url(video_id: Uuid) -> String {
    format!("/watch/{video_id}")
}

fn upload_temp_path() -> PathBuf {
    std::env::temp_dir().join(format!("hermes-upload-{}", Uuid::new_v4()))
}

async fn create_temp_file(path: &PathBuf) -> Result<File, ApiError> {
    File::create(path)
        .await
        .map_err(|error| ApiError::Internal(error.into()))
}

async fn cleanup_temp_file(path: &PathBuf) {
    if let Err(error) = fs::remove_file(path).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            warn!(path = %path.display(), error = %error, "failed to remove temp upload file");
        }
    }
}

async fn cleanup_object(state: &AppState, key: &str) {
    if let Err(error) = delete_object(&state.services.s3, &state.config.raw_bucket, key).await {
        warn!(bucket = %state.config.raw_bucket, key, error = %error, "failed to cleanup object");
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{header::CONTENT_LENGTH, HeaderValue};
    use uuid::Uuid;

    use super::{header_upload_too_large, shareable_url};

    #[test]
    fn content_length_precheck_allows_reasonable_multipart_overhead() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("1064"));

        let result = header_upload_too_large(&headers, 1024, 64).unwrap();
        assert!(!result);
    }

    #[test]
    fn content_length_precheck_rejects_large_requests() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("1200"));

        let result = header_upload_too_large(&headers, 1024, 64).unwrap();
        assert!(result);
    }

    #[test]
    fn shareable_url_points_to_watch_route() {
        assert_eq!(
            shareable_url(Uuid::nil()),
            "/watch/00000000-0000-0000-0000-000000000000"
        );
    }
}
