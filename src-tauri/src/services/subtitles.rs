use std::fs;
use std::path::Path;

use crate::error::AppError;
use crate::models::{ExportArtifact, ExportArtifactKind, SubtitleSegment, TranscriptResult};
use crate::services::gemini::client::GeminiSubtitleClient;
use crate::services::srt::render_srt;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleExtractionRequest {
    pub model: String,
    pub file_name: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

pub trait GeminiSubtitleClientLike: Send + Sync {
    fn extract_subtitles(
        &self,
        request: SubtitleExtractionRequest,
        export_dir: &str,
    ) -> Result<TranscriptResult, AppError>;
}

impl GeminiSubtitleClientLike for GeminiSubtitleClient {
    fn extract_subtitles(
        &self,
        request: SubtitleExtractionRequest,
        export_dir: &str,
    ) -> Result<TranscriptResult, AppError> {
        GeminiSubtitleClient::extract_subtitles(self, request, export_dir)
    }
}

pub struct SubtitleService {
    client: Box<dyn GeminiSubtitleClientLike>,
}

impl SubtitleService {
    pub fn new(client: Box<dyn GeminiSubtitleClientLike>) -> Self {
        Self { client }
    }

    pub fn production(api_key: String, timeout_ms: u64) -> Self {
        Self::new(Box::new(GeminiSubtitleClient::production(
            api_key, timeout_ms,
        )))
    }

    pub fn extract(
        &self,
        request: SubtitleExtractionRequest,
        export_dir: String,
    ) -> Result<TranscriptResult, AppError> {
        if request.data.is_empty() {
            return Err(AppError {
                code: "invalid_file".to_string(),
                message: "Audio or video file is required".to_string(),
                details: None,
            });
        }

        if request.data.len() > 200 * 1024 * 1024 {
            return Err(AppError {
                code: "file_too_large".to_string(),
                message: "File exceeds 200MB limit".to_string(),
                details: Some(format!("{} bytes", request.data.len())),
            });
        }

        self.client.extract_subtitles(request, &export_dir)
    }
}

pub fn write_srt_artifact(
    segments: &[SubtitleSegment],
    export_dir: &str,
    file_name: &str,
) -> Result<ExportArtifact, AppError> {
    fs::create_dir_all(export_dir).map_err(|error| AppError {
        code: "export_write_failed".to_string(),
        message: "Failed to prepare export directory".to_string(),
        details: Some(error.to_string()),
    })?;

    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("transcript");
    let export_path = Path::new(export_dir).join(format!("{stem}.srt"));
    let srt = render_srt(segments);

    fs::write(&export_path, srt).map_err(|error| AppError {
        code: "export_write_failed".to_string(),
        message: "Failed to write SRT export".to_string(),
        details: Some(error.to_string()),
    })?;

    Ok(ExportArtifact {
        path: export_path.to_string_lossy().to_string(),
        kind: ExportArtifactKind::Srt,
    })
}
