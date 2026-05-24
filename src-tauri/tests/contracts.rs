use ai_dev_installer::error::AppError;
use ai_dev_installer::models::{
    ApiKeyStatus, AppSettings, ChatMessage, ChatRole, ExportArtifact, ExportArtifactKind,
    GeminiModelOption, GeneratedImage, ImageGenerationResponse, SettingsConnectionResult,
    SubtitleSegment, TranscriptResult, WritableAppSettings,
};

#[test]
fn tauri_config_uses_explicit_content_security_policy() {
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri config should parse");

    let csp = config["app"]["security"]["csp"]
        .as_str()
        .expect("csp should be an explicit policy string");
    assert!(csp.contains("default-src 'self'"));
    assert!(csp.contains("img-src 'self' data: asset:"));
}

#[test]
fn app_error_has_machine_readable_shape() {
    let err = AppError {
        code: "missing_api_key".to_string(),
        message: "API key not configured".to_string(),
        details: None,
    };

    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"], "missing_api_key");
    assert_eq!(json["message"], "API key not configured");
    assert!(json["details"].is_null());
}

#[test]
fn transcript_result_is_serializable_with_shared_wire_shape() {
    let transcript = TranscriptResult {
        segments: vec![SubtitleSegment {
            start_ms: 0,
            end_ms: 1_500,
            text: "你好".to_string(),
        }],
        artifact: ExportArtifact {
            path: "C:/exports/sample.srt".to_string(),
            kind: ExportArtifactKind::Srt,
        },
    };

    let json = serde_json::to_value(&transcript).unwrap();
    assert_eq!(json["segments"][0]["startMs"], 0);
    assert_eq!(json["segments"][0]["endMs"], 1_500);
    assert_eq!(json["segments"][0]["text"], "你好");
    assert_eq!(json["artifact"]["path"], "C:/exports/sample.srt");
    assert_eq!(json["artifact"]["kind"], "srt");
}

#[test]
fn chat_message_role_serializes_as_shared_union_values() {
    let message = ChatMessage {
        role: ChatRole::Assistant,
        content: "hello".to_string(),
    };

    let json = serde_json::to_value(&message).unwrap();
    assert_eq!(json["role"], "assistant");
    assert_eq!(json["content"], "hello");
}

#[test]
fn app_settings_serializes_api_key_status_with_shared_values() {
    let settings = AppSettings {
        api_key_status: ApiKeyStatus::Configured,
        default_chat_model: "gemini-2.0-flash".to_string(),
        default_image_model: "gemini-2.0-flash-preview-image-generation".to_string(),
        default_export_dir: "C:/exports".to_string(),
        request_timeout_ms: 30_000,
    };

    let json = serde_json::to_value(&settings).unwrap();
    assert_eq!(json["apiKeyStatus"], "configured");
    assert_eq!(json["defaultChatModel"], "gemini-2.0-flash");
    assert_eq!(
        json["defaultImageModel"],
        "gemini-2.0-flash-preview-image-generation"
    );
    assert_eq!(json["defaultExportDir"], "C:/exports");
    assert_eq!(json["requestTimeoutMs"], 30_000);
}

#[test]
fn image_generation_response_serializes_camel_case_fields() {
    let response = ImageGenerationResponse {
        images: vec![GeneratedImage {
            mime_type: "image/png".to_string(),
            data: "iVBORw0KGgo=".to_string(),
        }],
    };

    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json["images"][0]["mimeType"], "image/png");
    assert_eq!(json["images"][0]["data"], "iVBORw0KGgo=");
}

#[test]
fn writable_app_settings_and_connection_result_use_shared_wire_shape() {
    let writable = WritableAppSettings {
        default_chat_model: "gemini-2.5-flash".to_string(),
        default_image_model: "imagen-3".to_string(),
        default_export_dir: "C:/exports".to_string(),
        request_timeout_ms: 15_000,
    };
    let connection = SettingsConnectionResult {
        ok: true,
        message: "Connection succeeded".to_string(),
    };

    let writable_json = serde_json::to_value(&writable).unwrap();
    let connection_json = serde_json::to_value(&connection).unwrap();

    assert_eq!(writable_json["defaultChatModel"], "gemini-2.5-flash");
    assert_eq!(writable_json["defaultImageModel"], "imagen-3");
    assert_eq!(writable_json["defaultExportDir"], "C:/exports");
    assert_eq!(writable_json["requestTimeoutMs"], 15_000);
    assert_eq!(connection_json["ok"], true);
    assert_eq!(connection_json["message"], "Connection succeeded");
}

#[test]
fn gemini_model_option_serializes_shared_model_catalog_shape() {
    let model = GeminiModelOption {
        model_id: "gemini-2.0-flash".to_string(),
        display_name: "Gemini 2.0 Flash".to_string(),
        supported_generation_methods: vec!["generateContent".to_string()],
        supports_chat: true,
        supports_image: false,
    };

    let json = serde_json::to_value(&model).unwrap();
    assert_eq!(json["modelId"], "gemini-2.0-flash");
    assert_eq!(json["displayName"], "Gemini 2.0 Flash");
    assert_eq!(json["supportedGenerationMethods"][0], "generateContent");
    assert_eq!(json["supportsChat"], true);
    assert_eq!(json["supportsImage"], false);
}
