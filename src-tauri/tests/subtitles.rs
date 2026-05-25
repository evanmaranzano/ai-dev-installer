use ai_dev_installer::error::AppError;
use ai_dev_installer::models::{ExportArtifactKind, SubtitleSegment, TranscriptResult};
use ai_dev_installer::services::subtitles::{
    validate_export_dir, write_srt_artifact, SubtitleExtractionRequest, SubtitleService,
};

struct FakeSubtitleClient {
    result: TranscriptResult,
}

impl FakeSubtitleClient {
    fn new(result: TranscriptResult) -> Self {
        Self { result }
    }
}

impl ai_dev_installer::services::subtitles::GeminiSubtitleClientLike for FakeSubtitleClient {
    fn extract_subtitles(
        &self,
        _request: SubtitleExtractionRequest,
        _export_dir: &str,
    ) -> Result<TranscriptResult, AppError> {
        Ok(self.result.clone())
    }
}

#[test]
fn returns_transcript_segments_and_artifact_from_fake_client() {
    let service = SubtitleService::new(Box::new(FakeSubtitleClient::new(TranscriptResult {
        segments: vec![
            SubtitleSegment {
                start_ms: 0,
                end_ms: 1500,
                text: "你好".to_string(),
            },
            SubtitleSegment {
                start_ms: 1500,
                end_ms: 3000,
                text: "世界".to_string(),
            },
        ],
        artifact: ai_dev_installer::models::ExportArtifact {
            path: "C:/exports/sample.srt".to_string(),
            kind: ExportArtifactKind::Srt,
        },
    })));

    let result = service
        .extract(
            SubtitleExtractionRequest {
                model: "gemini-2.0-flash".to_string(),
                file_name: "sample.wav".to_string(),
                mime_type: "audio/wav".to_string(),
                data: vec![1, 2, 3],
            },
            "C:/exports".to_string(),
        )
        .unwrap();

    assert_eq!(result.segments.len(), 2);
    assert_eq!(result.segments[0].text, "你好");
    assert_eq!(result.artifact.path, "C:/exports/sample.srt");
    assert_eq!(result.artifact.kind, ExportArtifactKind::Srt);
}

#[test]
fn rejects_high_risk_export_directories() {
    for path in [
        "",
        "relative/exports",
        "C:/",
        "C:/Windows/System32",
        "C:/Program Files/AI Dev Installer",
        "C:/PROGRA~1/AI Dev Installer",
        "C:/Windows./System32",
        "C:/ProgramData/Microsoft/Windows/Start Menu/Programs/Startup",
        "C:/ProgramData/Microsoft/Windows/Start Menu/Programs/Startup.",
        "C:/Users/Alice/AppData/Roaming/Microsoft/Windows/Start Menu/Programs/Startup",
        "C:/Users/Alice/AppData/Roaming/Microsoft/Windows/Start Menu/Programs/Startup ",
        "C:/exports/foo ",
        " C:/exports/foo",
        r"\\server\share",
        r"\\?\C:\exports",
        "C:/exports/../Windows",
    ] {
        let error = validate_export_dir(path).expect_err("path should be rejected");
        assert_eq!(error.code, "invalid_export_dir");
    }
}

#[test]
fn writes_srt_artifact_to_allowed_export_directory() {
    let export_dir = std::env::temp_dir().join("ai-dev-installer-subtitles-export-test");
    let _ = std::fs::remove_dir_all(&export_dir);

    let artifact = write_srt_artifact(
        &[SubtitleSegment {
            start_ms: 0,
            end_ms: 1500,
            text: "你好".to_string(),
        }],
        export_dir.to_str().expect("temp path should be utf8"),
        "sample.wav",
    )
    .unwrap();

    assert_eq!(artifact.kind, ExportArtifactKind::Srt);
    assert!(artifact.path.ends_with("sample.srt"));
    assert!(std::path::Path::new(&artifact.path).exists());

    let _ = std::fs::remove_dir_all(&export_dir);
}

#[test]
fn sanitizes_export_file_name_before_writing_srt_artifact() {
    let export_dir = std::env::temp_dir().join("ai-dev-installer-subtitles-export-name-test");
    let _ = std::fs::remove_dir_all(&export_dir);

    let artifact = write_srt_artifact(
        &[SubtitleSegment {
            start_ms: 0,
            end_ms: 1500,
            text: "你好".to_string(),
        }],
        export_dir.to_str().expect("temp path should be utf8"),
        "CON:?bad/name.wav",
    )
    .unwrap();

    assert_eq!(artifact.kind, ExportArtifactKind::Srt);
    assert!(artifact.path.ends_with("transcript.srt"));
    assert!(std::path::Path::new(&artifact.path).exists());

    let _ = std::fs::remove_dir_all(&export_dir);
}

#[cfg(target_os = "windows")]
#[test]
fn rejects_export_directory_after_reparse_point_resolution() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    let temp_root = std::env::temp_dir().join(format!("ai-dev-installer-subtitle-link-{unique}"));
    let target_dir = std::path::PathBuf::from(r"C:\Windows");
    let link_dir = temp_root.join("safe-looking-export");

    if !target_dir.exists() {
        return;
    }
    std::fs::create_dir_all(&temp_root).expect("temp root should be created");
    if std::os::windows::fs::symlink_dir(&target_dir, &link_dir).is_err() {
        let _ = std::fs::remove_dir_all(&temp_root);
        return;
    }

    let error = write_srt_artifact(
        &[SubtitleSegment {
            start_ms: 0,
            end_ms: 1500,
            text: "hello".to_string(),
        }],
        link_dir.to_str().expect("link path should be utf8"),
        "sample.wav",
    )
    .expect_err("resolved high-risk export dir should be rejected");

    assert_eq!(error.code, "invalid_export_dir");
    let _ = std::fs::remove_dir(&link_dir);
    let _ = std::fs::remove_dir_all(&temp_root);
}
