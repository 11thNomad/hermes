use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VideoStatus {
    Pending,
    Processing,
    Ready,
    Failed,
}

impl VideoStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "processing" => Some(Self::Processing),
            "ready" => Some(Self::Ready),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DetectedFormat {
    Mp4,
    Mkv,
    Webm,
    Mov,
}

impl DetectedFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mkv => "mkv",
            Self::Webm => "webm",
            Self::Mov => "mov",
        }
    }

    pub fn extension(self) -> &'static str {
        self.as_str()
    }

    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Mp4 => "video/mp4",
            Self::Mkv => "video/x-matroska",
            Self::Webm => "video/webm",
            Self::Mov => "video/quicktime",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "mp4" => Some(Self::Mp4),
            "mkv" => Some(Self::Mkv),
            "webm" => Some(Self::Webm),
            "mov" => Some(Self::Mov),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VideoRecord {
    pub id: Uuid,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub status: VideoStatus,
    pub raw_key: String,
    pub hls_manifest_key: Option<String>,
    pub error_msg: Option<String>,
    pub detected_format: DetectedFormat,
    pub attempt_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UploadVideoResponse {
    pub id: Uuid,
    pub shareable_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, ToSchema)]
pub struct VideoStatusResponse {
    pub id: Uuid,
    pub status: VideoStatus,
    pub raw_stream_url: String,
    pub hls_playlist_url: Option<String>,
    pub error_msg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, ToSchema)]
pub struct VideoListItem {
    pub id: Uuid,
    pub filename: String,
    pub status: VideoStatus,
    pub size_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscodeJob {
    pub video_id: Uuid,
    pub source_bucket: String,
    pub source_key: String,
    pub output_bucket: String,
    pub output_prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscodeDlqJob {
    pub video_id: Uuid,
    pub source_bucket: String,
    pub source_key: String,
    pub output_bucket: String,
    pub output_prefix: String,
    pub error_msg: String,
    pub attempt_count: i32,
    pub original_message_id: String,
}
