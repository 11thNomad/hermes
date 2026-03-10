use axum::{
    body::Body,
    extract::{Path, State},
    http::{
        header::{CONTENT_LENGTH, CONTENT_TYPE},
        HeaderValue, StatusCode,
    },
    response::Response,
    routing::get,
    Router,
};
use common::{db::get_video, storage::get_object};
use futures_util::stream;
use uuid::Uuid;

use crate::{routes::ApiError, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/videos/:id/hls/*path", get(stream_hls_asset))
}

#[utoipa::path(
    get,
    path = "/api/videos/{id}/hls/{path}",
    tag = "videos",
    params(
        ("id" = Uuid, Path, description = "Video identifier"),
        ("path" = String, Path, description = "Playlist or segment path under the video's HLS prefix")
    ),
    responses(
        (status = 200, description = "HLS playlist or segment asset"),
        (status = 404, description = "Video or HLS asset not found", body = crate::routes::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::routes::ErrorResponse)
    )
)]
pub(crate) async fn stream_hls_asset(
    State(state): State<AppState>,
    Path((video_id, asset_path)): Path<(Uuid, String)>,
) -> Result<Response, ApiError> {
    let video = get_video(&state.services.db, video_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("video `{video_id}` was not found")))?;
    let manifest_key = video
        .hls_manifest_key
        .as_deref()
        .ok_or_else(|| ApiError::NotFound(format!("video `{video_id}` has no HLS output")))?;
    if video.status != common::models::VideoStatus::Ready {
        return Err(ApiError::NotFound(format!(
            "video `{video_id}` is not ready for HLS playback"
        )));
    }

    let manifest_prefix = manifest_key
        .rsplit_once('/')
        .map(|(prefix, _)| prefix)
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("invalid HLS manifest key")))?;
    let normalized_path = asset_path.trim_start_matches('/');
    if normalized_path.is_empty() {
        return Err(ApiError::NotFound("HLS asset path is required".to_owned()));
    }
    let object_key = format!("{manifest_prefix}/{normalized_path}");

    let object = get_object(
        &state.services.s3,
        &state.config.hls_bucket,
        &object_key,
        None,
    )
    .await?;
    let content_length = object.content_length.unwrap_or_default().max(0) as u64;
    let body_stream = stream::try_unfold(object.body, |mut body| async move {
        match body.try_next().await {
            Ok(Some(chunk)) => Ok(Some((chunk, body))),
            Ok(None) => Ok(None),
            Err(error) => Err(std::io::Error::other(error)),
        }
    });

    let mut response = Response::new(Body::from_stream(body_stream));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static(hls_content_type(normalized_path)),
    );
    response.headers_mut().insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&content_length.to_string()).map_err(|_| {
            ApiError::Internal(anyhow::anyhow!("invalid content length `{content_length}`"))
        })?,
    );

    Ok(response)
}

fn hls_content_type(path: &str) -> &'static str {
    if path.ends_with(".m3u8") {
        "application/vnd.apple.mpegurl"
    } else if path.ends_with(".ts") {
        "video/mp2t"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::hls_content_type;

    #[test]
    fn infers_hls_content_types() {
        assert_eq!(
            hls_content_type("index.m3u8"),
            "application/vnd.apple.mpegurl"
        );
        assert_eq!(hls_content_type("segment_000.ts"), "video/mp2t");
        assert_eq!(hls_content_type("blob.bin"), "application/octet-stream");
    }
}
