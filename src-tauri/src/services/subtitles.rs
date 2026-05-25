use std::fs;
use std::path::{Component, Path};

use crate::error::AppError;
use crate::models::{ExportArtifact, ExportArtifactKind, SubtitleSegment, TranscriptResult};
use crate::services::gemini::client::GeminiSubtitleClient;
use crate::services::srt::render_srt;

const WINDOWS_STARTUP_DIR: &str = r"c:\programdata\microsoft\windows\start menu\programs\startup";
const WINDOWS_USER_STARTUP_SUFFIX: &str =
    r"\appdata\roaming\microsoft\windows\start menu\programs\startup";

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
        validate_export_dir(&export_dir)?;

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

pub fn validate_export_dir(export_dir: &str) -> Result<(), AppError> {
    let trimmed = export_dir.trim();
    if trimmed.is_empty() {
        return Err(invalid_export_dir(export_dir));
    }
    if trimmed != export_dir {
        return Err(invalid_export_dir(export_dir));
    }

    let normalized = trimmed.replace('/', r"\");
    let lower = normalized.trim_end_matches('\\').to_ascii_lowercase();

    if normalized.starts_with(r"\\") || !Path::new(trimmed).is_absolute() {
        return Err(invalid_export_dir(export_dir));
    }

    if has_unsafe_windows_component(&lower) {
        return Err(invalid_export_dir(export_dir));
    }

    if Path::new(trimmed)
        .components()
        .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(invalid_export_dir(export_dir));
    }

    if is_drive_root(&lower)
        || path_has_prefix(&lower, r"c:\windows")
        || path_has_prefix(&lower, r"c:\program files")
        || path_has_prefix(&lower, r"c:\program files (x86)")
        || path_has_prefix(&lower, WINDOWS_STARTUP_DIR)
        || path_has_suffix_or_child(&lower, WINDOWS_USER_STARTUP_SUFFIX)
    {
        return Err(invalid_export_dir(export_dir));
    }

    Ok(())
}

fn path_has_prefix(path: &str, prefix: &str) -> bool {
    path == prefix || path.starts_with(&format!(r"{prefix}\"))
}

fn path_has_suffix_or_child(path: &str, suffix: &str) -> bool {
    path.ends_with(suffix) || path.contains(&format!(r"{suffix}\"))
}

fn has_unsafe_windows_component(path: &str) -> bool {
    path.split('\\').any(|component| {
        component.ends_with(['.', ' ']) || matches!(component, "progra~1" | "progra~2")
    })
}

fn is_drive_root(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() == 2 && bytes[1] == b':'
}

fn invalid_export_dir(export_dir: &str) -> AppError {
    AppError {
        code: "invalid_export_dir".to_string(),
        message: "Export directory is not allowed".to_string(),
        details: Some(export_dir.to_string()),
    }
}

pub fn write_srt_artifact(
    segments: &[SubtitleSegment],
    export_dir: &str,
    file_name: &str,
) -> Result<ExportArtifact, AppError> {
    validate_export_dir(export_dir)?;

    fs::create_dir_all(export_dir).map_err(|error| AppError {
        code: "export_write_failed".to_string(),
        message: "Failed to prepare export directory".to_string(),
        details: Some(error.to_string()),
    })?;

    let export_dir = fs::canonicalize(export_dir).map_err(|error| AppError {
        code: "export_write_failed".to_string(),
        message: "Failed to resolve export directory".to_string(),
        details: Some(error.to_string()),
    })?;
    validate_export_dir(&normalized_export_dir_for_validation(&export_dir))?;

    let stem = sanitized_export_stem(file_name);
    let export_path = export_dir.join(format!("{stem}.srt"));
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

fn normalized_export_dir_for_validation(path: &Path) -> String {
    let raw = path.to_string_lossy();
    raw.strip_prefix(r"\\?\").unwrap_or(&raw).to_string()
}

fn sanitized_export_stem(file_name: &str) -> String {
    if file_name.contains(['/', '\\', ':']) {
        return "transcript".to_string();
    }

    let Some(stem) = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return "transcript".to_string();
    };

    let sanitized = stem
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | ' ') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches(['.', ' ', '_'])
        .to_string();

    if sanitized.is_empty() || is_reserved_windows_file_stem(&sanitized) {
        "transcript".to_string()
    } else {
        sanitized
    }
}

fn is_reserved_windows_file_stem(stem: &str) -> bool {
    let lower = stem.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "con"
            | "prn"
            | "aux"
            | "nul"
            | "com1"
            | "com2"
            | "com3"
            | "com4"
            | "com5"
            | "com6"
            | "com7"
            | "com8"
            | "com9"
            | "lpt1"
            | "lpt2"
            | "lpt3"
            | "lpt4"
            | "lpt5"
            | "lpt6"
            | "lpt7"
            | "lpt8"
            | "lpt9"
    )
}
