mod docs;
mod routes;
mod state;

use anyhow::{Context, Result};
use common::{init_tracing, initialize};
use http::{
    header::{ACCEPT, CONTENT_TYPE, RANGE},
    HeaderValue, Method,
};
use state::AppState;
use tokio::net::TcpListener;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer},
};
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let (config, services) = initialize("api").await?;
    let listener = TcpListener::bind(config.api_bind_addr)
        .await
        .with_context(|| format!("failed to bind API on {}", config.api_bind_addr))?;

    let cors = build_cors(&config.cors_allowed_origins);
    info!(
        base_url = %config.base_url,
        cors_allowed_origins = ?config.cors_allowed_origins,
        "api configuration loaded"
    );
    let state = AppState { config, services };
    let app = routes::router()
        .merge(docs::router())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(cors)
        .with_state(state);

    let local_addr = listener
        .local_addr()
        .context("failed to read listener address")?;
    info!(address = %local_addr, "api server listening");
    axum::serve(listener, app)
        .await
        .context("api server failed")
}

fn build_cors(origins: &[String]) -> CorsLayer {
    if origins.iter().any(|origin| origin == "*") {
        return CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);
    }

    let allowed = origins
        .iter()
        .filter_map(|origin| origin.parse::<HeaderValue>().ok())
        .collect::<Vec<_>>();

    CorsLayer::new()
        .allow_origin(allowed)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([ACCEPT, CONTENT_TYPE, RANGE])
}
