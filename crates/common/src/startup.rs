use anyhow::{Context, Result};
use aws_sdk_s3::Client as S3Client;
use redis::Client as RedisClient;
use sqlx::{migrate::Migrator, postgres::PgPoolOptions, PgPool};
use tracing::info;

use crate::{
    config::AppConfig,
    queue::ensure_consumer_group,
    storage::{build_s3_client, ensure_bucket},
};

static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

#[derive(Clone)]
pub struct SharedServices {
    pub db: PgPool,
    pub redis: RedisClient,
    pub s3: S3Client,
}

pub async fn initialize(app_name: &str) -> Result<(AppConfig, SharedServices)> {
    let config = AppConfig::from_env(app_name)?;
    info!(
        app_name = %config.app_name,
        database_url = %config.database_url,
        redis_url = %config.redis_url,
        s3_endpoint = %config.s3_endpoint,
        "initializing shared services"
    );
    let db = connect_database(&config).await?;
    let redis = RedisClient::open(config.redis_url.clone()).context("invalid REDIS_URL")?;
    let s3 = build_s3_client(&config).await?;

    bootstrap(&config, &db, &redis, &s3).await?;

    Ok((config, SharedServices { db, redis, s3 }))
}

async fn connect_database(config: &AppConfig) -> Result<PgPool> {
    info!(
        max_connections = config.db_max_connections,
        "connecting to postgres"
    );
    PgPoolOptions::new()
        .max_connections(config.db_max_connections)
        .connect(&config.database_url)
        .await
        .context("failed to connect to Postgres")
}

async fn bootstrap(
    config: &AppConfig,
    db: &PgPool,
    redis: &RedisClient,
    s3: &S3Client,
) -> Result<()> {
    if config.run_migrations {
        info!("running database migrations");
        MIGRATOR.run(db).await.context("failed to run migrations")?;
        info!("database migrations completed");
    } else {
        info!("database migrations skipped");
    }

    info!(bucket = %config.raw_bucket, "ensuring raw bucket");
    ensure_bucket(s3, &config.raw_bucket).await?;
    info!(bucket = %config.hls_bucket, "ensuring hls bucket");
    ensure_bucket(s3, &config.hls_bucket).await?;
    info!(
        stream = %config.transcode_stream,
        group = %config.transcode_consumer_group,
        "ensuring redis consumer group"
    );
    ensure_consumer_group(
        redis,
        &config.transcode_stream,
        &config.transcode_consumer_group,
    )
    .await?;
    info!("shared bootstrap complete");

    Ok(())
}
