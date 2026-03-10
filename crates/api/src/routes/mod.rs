pub(crate) mod events;
pub(crate) mod health;
pub(crate) mod hls;
pub(crate) mod status;
pub(crate) mod stream;
pub(crate) mod upload;

use axum::{
    http::{header::CONTENT_RANGE, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json, Router,
};
use serde::Serialize;
use thiserror::Error;
use utoipa::ToSchema;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .merge(events::router())
        .merge(hls::router())
        .merge(status::router())
        .merge(upload::router())
        .merge(stream::router())
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    PayloadTooLarge(String),
    #[error("{0}")]
    UnsupportedMediaType(String),
    #[error("range is unsatisfiable")]
    RangeNotSatisfiable(u64),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message.clone()),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, message.clone()),
            Self::PayloadTooLarge(message) => (StatusCode::PAYLOAD_TOO_LARGE, message.clone()),
            Self::UnsupportedMediaType(message) => {
                (StatusCode::UNSUPPORTED_MEDIA_TYPE, message.clone())
            }
            Self::RangeNotSatisfiable(total_size) => {
                tracing::warn!(
                    status = %StatusCode::RANGE_NOT_SATISFIABLE,
                    error = "range is unsatisfiable",
                    total_size,
                    "request failed"
                );
                let mut response = (
                    StatusCode::RANGE_NOT_SATISFIABLE,
                    Json(ErrorResponse {
                        error: self.to_string(),
                    }),
                )
                    .into_response();
                let content_range = format!("bytes */{total_size}");
                if let Ok(value) = HeaderValue::from_str(&content_range) {
                    response.headers_mut().insert(CONTENT_RANGE, value);
                }
                return response;
            }
            Self::Internal(error) => {
                tracing::error!(
                    status = %StatusCode::INTERNAL_SERVER_ERROR,
                    error = %error,
                    "request failed"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_owned(),
                )
            }
        };

        tracing::warn!(status = %status, error = %message, "request failed");
        (status, Json(ErrorResponse { error: message })).into_response()
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ErrorResponse {
    pub(crate) error: String,
}
