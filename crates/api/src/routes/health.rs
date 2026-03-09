use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(root))
        .route("/healthz", get(healthcheck))
        .route("/api/healthz", get(api_healthcheck))
}

#[utoipa::path(
    get,
    path = "/",
    tag = "system",
    responses((status = 200, description = "Service info", body = HealthResponse))
)]
pub(crate) async fn root(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        service: state.config.app_name,
        kind: "info",
        ok: true,
    })
}

#[utoipa::path(
    get,
    path = "/healthz",
    tag = "system",
    responses((status = 200, description = "Health check", body = HealthResponse))
)]
pub(crate) async fn healthcheck(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(health_payload(state))
}

#[utoipa::path(
    get,
    path = "/api/healthz",
    tag = "system",
    responses((status = 200, description = "API health check", body = HealthResponse))
)]
pub(crate) async fn api_healthcheck(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(health_payload(state))
}

fn health_payload(state: AppState) -> HealthResponse {
    HealthResponse {
        service: state.config.app_name,
        kind: "health",
        ok: true,
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct HealthResponse {
    pub(crate) service: String,
    pub(crate) kind: &'static str,
    pub(crate) ok: bool,
}

#[cfg(test)]
mod tests {
    use aws_credential_types::Credentials;
    use aws_sdk_s3::Client as S3Client;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use common::{AppConfig, SharedServices};
    use redis::Client as RedisClient;
    use sqlx::{postgres::PgPoolOptions, PgPool};
    use tower::util::ServiceExt;

    use crate::{routes, state::AppState};

    #[tokio::test]
    async fn root_returns_info_payload() {
        let response = app()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(
            body,
            serde_json::json!({"service":"api","kind":"info","ok":true})
        );
    }

    #[tokio::test]
    async fn health_endpoints_return_health_payload() {
        for uri in ["/healthz", "/api/healthz"] {
            let response = app()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .expect("request should succeed");

            assert_eq!(
                response.status(),
                StatusCode::OK,
                "unexpected status for {uri}"
            );
            let body = json_body(response).await;
            assert_eq!(
                body,
                serde_json::json!({"service":"api","kind":"health","ok":true})
            );
        }
    }

    fn app() -> axum::Router {
        routes::router().with_state(test_state())
    }

    fn test_state() -> AppState {
        AppState {
            config: AppConfig {
                app_name: "api".to_owned(),
                api_bind_addr: "127.0.0.1:8080".parse().unwrap(),
                base_url: "http://localhost:8080".parse().unwrap(),
                cors_allowed_origins: vec!["http://localhost:5173".to_owned()],
                database_url: "postgres://postgres:postgres@localhost/hermes".to_owned(),
                redis_url: "redis://127.0.0.1:6379".to_owned(),
                s3_endpoint: "http://127.0.0.1:9000".to_owned(),
                s3_region: "us-east-1".to_owned(),
                s3_access_key: "minioadmin".to_owned(),
                s3_secret_key: "minioadmin".to_owned(),
                raw_bucket: "videos-raw".to_owned(),
                hls_bucket: "videos-hls".to_owned(),
                transcode_stream: "transcode_jobs".to_owned(),
                transcode_consumer_group: "workers".to_owned(),
                max_upload_bytes: 1_073_741_824,
                job_visibility_timeout_secs: 300,
                run_migrations: true,
                worker_poll_interval_ms: 5_000,
                db_max_connections: 5,
            },
            services: SharedServices {
                db: test_pool(),
                redis: RedisClient::open("redis://127.0.0.1:6379").unwrap(),
                s3: test_s3_client(),
            },
        }
    }

    fn test_pool() -> PgPool {
        PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/hermes")
            .unwrap()
    }

    fn test_s3_client() -> S3Client {
        let config = aws_sdk_s3::config::Builder::new()
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .credentials_provider(Credentials::new(
                "minioadmin",
                "minioadmin",
                None,
                None,
                "tests",
            ))
            .endpoint_url("http://127.0.0.1:9000")
            .region(aws_sdk_s3::config::Region::new("us-east-1"))
            .force_path_style(true)
            .build();

        S3Client::from_conf(config)
    }

    async fn json_body(response: axum::response::Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should read");
        serde_json::from_slice(&bytes).expect("body should be json")
    }
}
