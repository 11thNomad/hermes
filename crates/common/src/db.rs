use anyhow::{anyhow, Context, Result};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::models::{DetectedFormat, VideoRecord, VideoStatus};

#[derive(Debug, Clone)]
pub struct NewVideo {
    pub id: Uuid,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub status: VideoStatus,
    pub raw_key: String,
    pub detected_format: DetectedFormat,
}

pub async fn insert_video(pool: &PgPool, video: &NewVideo) -> Result<VideoRecord> {
    let row = sqlx::query(
        r#"
        INSERT INTO videos (
            id,
            filename,
            mime_type,
            size_bytes,
            status,
            raw_key,
            detected_format
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING
            id,
            filename,
            mime_type,
            size_bytes,
            status,
            raw_key,
            hls_manifest_key,
            error_msg,
            detected_format,
            attempt_count
        "#,
    )
    .bind(video.id)
    .bind(&video.filename)
    .bind(&video.mime_type)
    .bind(video.size_bytes)
    .bind(video.status.as_str())
    .bind(&video.raw_key)
    .bind(video.detected_format.as_str())
    .fetch_one(pool)
    .await
    .context("failed to insert video")?;

    map_video_row(row)
}

pub async fn get_video(pool: &PgPool, id: Uuid) -> Result<Option<VideoRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            id,
            filename,
            mime_type,
            size_bytes,
            status,
            raw_key,
            hls_manifest_key,
            error_msg,
            detected_format,
            attempt_count
        FROM videos
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("failed to load video `{id}`"))?;

    row.map(map_video_row).transpose()
}

pub async fn delete_video(pool: &PgPool, id: Uuid) -> Result<()> {
    sqlx::query("DELETE FROM videos WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .with_context(|| format!("failed to delete video `{id}`"))?;

    Ok(())
}

fn map_video_row(row: sqlx::postgres::PgRow) -> Result<VideoRecord> {
    let status = row.get::<String, _>("status");
    let detected_format = row.get::<String, _>("detected_format");

    Ok(VideoRecord {
        id: row.get("id"),
        filename: row.get("filename"),
        mime_type: row.get("mime_type"),
        size_bytes: row.get("size_bytes"),
        status: VideoStatus::parse(&status)
            .ok_or_else(|| anyhow!("unknown video status `{status}`"))?,
        raw_key: row.get("raw_key"),
        hls_manifest_key: row.get("hls_manifest_key"),
        error_msg: row.get("error_msg"),
        detected_format: DetectedFormat::parse(&detected_format)
            .ok_or_else(|| anyhow!("unknown detected format `{detected_format}`"))?,
        attempt_count: row.get("attempt_count"),
    })
}
