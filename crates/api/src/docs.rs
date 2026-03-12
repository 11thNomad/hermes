use axum::Router;
use common::models::{UploadVideoResponse, VideoListItem, VideoRecord, VideoStatusResponse};
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

use crate::state::AppState;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::routes::health::root,
        crate::routes::health::healthcheck,
        crate::routes::health::api_healthcheck,
        crate::routes::upload::init_direct_upload,
        crate::routes::upload::complete_direct_upload,
        crate::routes::upload::upload_video,
        crate::routes::upload::list_videos,
        crate::routes::status::video_status,
        crate::routes::events::video_events,
        crate::routes::hls::stream_hls_asset,
        crate::routes::stream::stream_video
    ),
    components(
        schemas(
            crate::routes::ErrorResponse,
            crate::routes::health::HealthResponse,
            UploadVideoResponse,
            VideoListItem,
            VideoRecord,
            VideoStatusResponse,
            crate::routes::upload::DirectUploadInitRequest,
            crate::routes::upload::DirectUploadInitResponse,
            crate::routes::upload::DirectUploadCompleteRequest,
            UploadVideoRequest
        )
    ),
    tags(
        (name = "system", description = "Health and service metadata endpoints"),
        (name = "videos", description = "Video upload and playback endpoints")
    )
)]
struct ApiDoc;

#[derive(Debug, ToSchema)]
#[allow(dead_code)]
pub(crate) struct UploadVideoRequest {
    #[schema(value_type = String, format = Binary)]
    pub(crate) video: String,
}

pub fn router() -> Router<AppState> {
    SwaggerUi::new("/swagger-ui")
        .url("/api-doc/openapi.json", ApiDoc::openapi())
        .into()
}
