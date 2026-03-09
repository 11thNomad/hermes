use std::{convert::Infallible, time::Duration};

use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Router,
};
use common::{db::get_video, models::VideoStatusResponse};
use futures_util::stream;
use tracing::warn;
use uuid::Uuid;

use crate::{
    routes::{status::build_status_response, ApiError},
    state::AppState,
};

const STATUS_POLL_INTERVAL: Duration = Duration::from_millis(250);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(5);

pub fn router() -> Router<AppState> {
    Router::new().route("/api/videos/:id/events", get(video_events))
}

#[utoipa::path(
    get,
    path = "/api/videos/{id}/events",
    tag = "videos",
    params(
        ("id" = Uuid, Path, description = "Video identifier")
    ),
    responses(
        (status = 200, description = "Server-sent stream of video status changes", content_type = "text/event-stream"),
        (status = 404, description = "Video not found", body = crate::routes::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::routes::ErrorResponse)
    )
)]
pub(crate) async fn video_events(
    State(state): State<AppState>,
    Path(video_id): Path<Uuid>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let video = get_video(&state.services.db, video_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("video `{video_id}` was not found")))?;
    let initial_payload = build_status_response(&state.config.base_url, &video);

    let event_stream = stream::unfold(
        EventStreamState {
            state,
            video_id,
            last_payload: None,
            next_payload: Some(initial_payload),
        },
        next_event,
    );

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(KEEPALIVE_INTERVAL)
            .text("keepalive"),
    ))
}

struct EventStreamState {
    state: AppState,
    video_id: Uuid,
    last_payload: Option<VideoStatusResponse>,
    next_payload: Option<VideoStatusResponse>,
}

async fn next_event(
    mut state: EventStreamState,
) -> Option<(Result<Event, Infallible>, EventStreamState)> {
    loop {
        let payload = if let Some(payload) = state.next_payload.take() {
            payload
        } else {
            tokio::time::sleep(STATUS_POLL_INTERVAL).await;

            let video = match get_video(&state.state.services.db, state.video_id).await {
                Ok(Some(video)) => video,
                Ok(None) => return None,
                Err(error) => {
                    warn!(video_id = %state.video_id, error = %error, "failed to poll video status for sse");
                    return None;
                }
            };
            build_status_response(&state.state.config.base_url, &video)
        };

        if state.last_payload.as_ref() == Some(&payload) {
            continue;
        }

        state.last_payload = Some(payload.clone());
        let event = match Event::default().event("status").json_data(&payload) {
            Ok(event) => event,
            Err(error) => {
                warn!(video_id = %state.video_id, error = %error, "failed to serialize video status event");
                return None;
            }
        };

        return Some((Ok(event), state));
    }
}

#[cfg(test)]
mod tests {
    use common::models::{VideoStatus, VideoStatusResponse};
    use uuid::Uuid;

    #[test]
    fn status_payload_detects_meaningful_change() {
        let baseline = VideoStatusResponse {
            id: Uuid::nil(),
            status: VideoStatus::Processing,
            raw_stream_url:
                "http://localhost:8080/api/videos/00000000-0000-0000-0000-000000000000/stream"
                    .to_owned(),
            hls_playlist_url: None,
            error_msg: None,
        };
        let updated = VideoStatusResponse {
            status: VideoStatus::Ready,
            hls_playlist_url: Some(
                "http://localhost:8080/api/videos/00000000-0000-0000-0000-000000000000/hls/index.m3u8"
                    .to_owned(),
            ),
            ..baseline.clone()
        };

        assert_ne!(baseline, updated);
    }
}
