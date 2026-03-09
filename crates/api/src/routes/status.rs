use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use common::{
    db::get_video,
    models::{VideoRecord, VideoStatus, VideoStatusResponse},
};
use url::Url;
use uuid::Uuid;

use crate::{routes::ApiError, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/videos/:id/status", get(video_status))
}

#[utoipa::path(
    get,
    path = "/api/videos/{id}/status",
    tag = "videos",
    params(
        ("id" = Uuid, Path, description = "Video identifier")
    ),
    responses(
        (status = 200, description = "Video processing status", body = VideoStatusResponse),
        (status = 404, description = "Video not found", body = crate::routes::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::routes::ErrorResponse)
    )
)]
pub(crate) async fn video_status(
    State(state): State<AppState>,
    Path(video_id): Path<Uuid>,
) -> Result<Json<VideoStatusResponse>, ApiError> {
    let video = get_video(&state.services.db, video_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("video `{video_id}` was not found")))?;

    Ok(Json(build_status_response(&state.config.base_url, &video)))
}

fn build_status_response(base_url: &Url, video: &VideoRecord) -> VideoStatusResponse {
    VideoStatusResponse {
        id: video.id,
        status: video.status,
        raw_stream_url: join_api_url(base_url, &format!("/api/videos/{}/stream", video.id)),
        hls_playlist_url: video.hls_manifest_key.as_ref().and_then(|_| {
            (video.status == VideoStatus::Ready).then(|| {
                join_api_url(
                    base_url,
                    &format!("/api/videos/{}/hls/index.m3u8", video.id),
                )
            })
        }),
        error_msg: video.error_msg.clone(),
    }
}

fn join_api_url(base_url: &Url, path: &str) -> String {
    base_url
        .join(path)
        .expect("status API paths should be valid URLs")
        .to_string()
}

#[cfg(test)]
mod tests {
    use common::models::{DetectedFormat, VideoRecord, VideoStatus};
    use uuid::Uuid;

    use super::build_status_response;

    #[test]
    fn builds_status_payload_for_pending_video() {
        let video = sample_video(VideoStatus::Pending, None, None);
        let response = build_status_response(&"http://localhost:8080".parse().unwrap(), &video);

        assert_eq!(response.id, video.id);
        assert_eq!(response.status, VideoStatus::Pending);
        assert_eq!(
            response.raw_stream_url,
            format!("http://localhost:8080/api/videos/{}/stream", video.id)
        );
        assert_eq!(response.hls_playlist_url, None);
        assert_eq!(response.error_msg, None);
    }

    #[test]
    fn builds_status_payload_for_ready_video() {
        let video = sample_video(
            VideoStatus::Ready,
            Some("video-id/hls/index.m3u8".to_owned()),
            None,
        );
        let response = build_status_response(&"http://localhost:8080".parse().unwrap(), &video);

        assert_eq!(
            response.hls_playlist_url,
            Some(format!(
                "http://localhost:8080/api/videos/{}/hls/index.m3u8",
                video.id
            ))
        );
    }

    #[test]
    fn includes_error_message_for_failed_video() {
        let video = sample_video(
            VideoStatus::Failed,
            None,
            Some("ffprobe rejected the media".to_owned()),
        );
        let response = build_status_response(&"http://localhost:8080".parse().unwrap(), &video);

        assert_eq!(response.status, VideoStatus::Failed);
        assert_eq!(
            response.error_msg,
            Some("ffprobe rejected the media".to_owned())
        );
    }

    fn sample_video(
        status: VideoStatus,
        hls_manifest_key: Option<String>,
        error_msg: Option<String>,
    ) -> VideoRecord {
        VideoRecord {
            id: Uuid::nil(),
            filename: "sample.webm".to_owned(),
            mime_type: "video/webm".to_owned(),
            size_bytes: 42,
            status,
            raw_key: "raw/sample.webm".to_owned(),
            hls_manifest_key,
            error_msg,
            detected_format: DetectedFormat::Webm,
            attempt_count: 1,
        }
    }
}
