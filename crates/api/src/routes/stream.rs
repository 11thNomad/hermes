use axum::{
    body::Body,
    extract::{Path, State},
    http::{
        header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::Response,
    routing::get,
    Router,
};
use common::{
    db::get_video,
    range::{parse_range_header, RangeError},
    storage::get_object,
};
use futures_util::stream;
use uuid::Uuid;

use crate::{routes::ApiError, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/videos/:id/stream", get(stream_video))
}

#[utoipa::path(
    get,
    path = "/api/videos/{id}/stream",
    tag = "videos",
    params(
        ("id" = Uuid, Path, description = "Video identifier"),
        ("Range" = Option<String>, Header, description = "Optional HTTP range request header")
    ),
    responses(
        (status = 200, description = "Full raw video stream", body = String, content_type = "application/octet-stream"),
        (status = 206, description = "Partial raw video stream", body = String, content_type = "application/octet-stream"),
        (status = 400, description = "Malformed Range header", body = crate::routes::ErrorResponse),
        (status = 404, description = "Video not found", body = crate::routes::ErrorResponse),
        (status = 416, description = "Range not satisfiable", body = crate::routes::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::routes::ErrorResponse)
    )
)]
pub(crate) async fn stream_video(
    State(state): State<AppState>,
    Path(video_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let video = get_video(&state.services.db, video_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("video `{video_id}` was not found")))?;

    let total_size = video.size_bytes.max(0) as u64;
    let requested_range = headers.get(RANGE).map(|value| {
        value
            .to_str()
            .map_err(|_| ApiError::BadRequest("invalid Range header".to_owned()))
    });
    let requested_range = requested_range.transpose()?;
    let parsed_range = requested_range
        .map(|value| parse_range_header(value, total_size))
        .transpose()
        .map_err(|error| match error {
            RangeError::Unsatisfiable => ApiError::RangeNotSatisfiable(total_size),
            RangeError::Malformed | RangeError::MultipleRanges => {
                ApiError::BadRequest(error.to_string())
            }
        })?;

    let object = get_object(
        &state.services.s3,
        &state.config.raw_bucket,
        &video.raw_key,
        parsed_range
            .as_ref()
            .map(|range| range.to_header_value())
            .as_deref(),
    )
    .await?;
    let content_length = object.content_length.unwrap_or(video.size_bytes).max(0) as u64;
    let content_type = object
        .content_type
        .clone()
        .unwrap_or_else(|| video.mime_type.clone());
    let body_stream = stream::try_unfold(object.body, |mut body| async move {
        match body.try_next().await {
            Ok(Some(chunk)) => Ok(Some((chunk, body))),
            Ok(None) => Ok(None),
            Err(error) => Err(std::io::Error::other(error)),
        }
    });

    let mut response = Response::new(Body::from_stream(body_stream));
    *response.status_mut() = if parsed_range.is_some() {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    response
        .headers_mut()
        .insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&content_type).map_err(|_| {
            ApiError::Internal(anyhow::anyhow!("invalid content type `{content_type}`"))
        })?,
    );
    response.headers_mut().insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&content_length.to_string()).map_err(|_| {
            ApiError::Internal(anyhow::anyhow!("invalid content length `{content_length}`"))
        })?,
    );

    if let Some(range) = parsed_range {
        response.headers_mut().insert(
            CONTENT_RANGE,
            HeaderValue::from_str(&range.to_content_range(total_size))
                .map_err(|_| ApiError::Internal(anyhow::anyhow!("invalid content range")))?,
        );
    }

    Ok(response)
}
