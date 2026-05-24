use crate::error::{sanitize_remote_error_details, AppError};
use crate::models::{
    ChatMessage, ChatResponse, ChatRole, GeneratedImage, ImageGenerationResponse, SubtitleSegment,
    TranscriptResult,
};
use crate::services::image::ImageRequest;
use crate::services::subtitles::{write_srt_artifact, SubtitleExtractionRequest};

#[derive(Debug, Clone)]
pub struct GeminiTransportRequest {
    pub model: String,
    pub prompt: String,
    pub history: Vec<ChatMessage>,
}

#[derive(Debug, Clone)]
pub struct GeminiTransportResponse {
    pub reply_text: String,
}

pub trait GeminiTransport: Send + Sync {
    fn send_chat(
        &self,
        request: GeminiTransportRequest,
    ) -> Result<GeminiTransportResponse, AppError>;
}

#[derive(Debug, Clone)]
pub struct GeminiImageTransportRequest {
    pub model: String,
    pub prompt: String,
    pub count: u32,
    pub aspect_ratio: String,
}

#[derive(Debug, Clone)]
pub struct GeminiImageTransportResponse {
    pub images: Vec<GeneratedImage>,
}

pub trait GeminiImageTransport: Send + Sync {
    fn send_image(
        &self,
        request: GeminiImageTransportRequest,
    ) -> Result<GeminiImageTransportResponse, AppError>;
}

#[derive(Debug, Clone)]
pub struct GeminiSubtitleTransportRequest {
    pub model: String,
    pub file_name: String,
    pub mime_type: String,
    pub data: Vec<u8>,
    pub export_dir: String,
}

pub trait GeminiSubtitleTransport: Send + Sync {
    fn extract_subtitles(
        &self,
        request: GeminiSubtitleTransportRequest,
    ) -> Result<TranscriptResult, AppError>;
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct GenerateContentRequest {
    contents: Vec<GenerateContentContent>,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct GenerateImageRequest {
    contents: Vec<GenerateContentContent>,
    generation_config: ImageGenerationConfig,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct GenerateSubtitleRequest {
    contents: Vec<GenerateContentContent>,
    generation_config: SubtitleGenerationConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct GenerateContentContent {
    role: String,
    parts: Vec<GenerateContentPart>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct GenerateContentPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inline_data: Option<InlineData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_data: Option<FileData>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct InlineData {
    mime_type: String,
    data: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct FileData {
    mime_type: String,
    file_uri: String,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ImageGenerationConfig {
    response_modalities: Vec<String>,
    response_format: ImageResponseFormat,
    candidate_count: u32,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ImageResponseFormat {
    image: ImageResponseFormatImage,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ImageResponseFormatImage {
    aspect_ratio: String,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct SubtitleGenerationConfig {
    response_mime_type: String,
    response_schema: SubtitleResponseSchema,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct SubtitleResponseSchema {
    #[serde(rename = "type")]
    schema_type: String,
    properties: serde_json::Value,
    required: Vec<String>,
    property_ordering: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct GenerateContentResponse {
    #[serde(default)]
    candidates: Vec<GenerateContentCandidate>,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct GenerateContentCandidate {
    content: Option<GenerateContentContent>,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct GeminiFileRecord {
    name: String,
    uri: String,
    state: String,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct GeminiFileEnvelope {
    file: GeminiFileRecord,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct SubtitleSegmentsEnvelope {
    segments: Vec<SubtitleSegment>,
}

fn build_generate_content_request(request: &GeminiTransportRequest) -> GenerateContentRequest {
    let mut contents = request
        .history
        .iter()
        .map(|message| GenerateContentContent {
            role: match message.role {
                ChatRole::System => "user".to_string(),
                ChatRole::User => "user".to_string(),
                ChatRole::Assistant => "model".to_string(),
            },
            parts: vec![GenerateContentPart {
                text: Some(message.content.clone()),
                inline_data: None,
                file_data: None,
            }],
        })
        .collect::<Vec<_>>();

    contents.push(GenerateContentContent {
        role: "user".to_string(),
        parts: vec![GenerateContentPart {
            text: Some(request.prompt.clone()),
            inline_data: None,
            file_data: None,
        }],
    });

    GenerateContentRequest { contents }
}

fn build_generate_image_request(request: &GeminiImageTransportRequest) -> GenerateImageRequest {
    GenerateImageRequest {
        contents: vec![GenerateContentContent {
            role: "user".to_string(),
            parts: vec![GenerateContentPart {
                text: Some(request.prompt.clone()),
                inline_data: None,
                file_data: None,
            }],
        }],
        generation_config: ImageGenerationConfig {
            response_modalities: vec!["TEXT".to_string(), "IMAGE".to_string()],
            response_format: ImageResponseFormat {
                image: ImageResponseFormatImage {
                    aspect_ratio: request.aspect_ratio.clone(),
                },
            },
            candidate_count: request.count,
        },
    }
}

fn build_generate_subtitle_request(
    model_instruction: &str,
    file_uri: &str,
    mime_type: &str,
) -> GenerateSubtitleRequest {
    GenerateSubtitleRequest {
        contents: vec![GenerateContentContent {
            role: "user".to_string(),
            parts: vec![
                GenerateContentPart {
                    text: Some(model_instruction.to_string()),
                    inline_data: None,
                    file_data: None,
                },
                GenerateContentPart {
                    text: None,
                    inline_data: None,
                    file_data: Some(FileData {
                        mime_type: mime_type.to_string(),
                        file_uri: file_uri.to_string(),
                    }),
                },
            ],
        }],
        generation_config: SubtitleGenerationConfig {
            response_mime_type: "application/json".to_string(),
            response_schema: SubtitleResponseSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({
                    "segments": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "startMs": { "type": "integer" },
                                "endMs": { "type": "integer" },
                                "text": { "type": "string" }
                            },
                            "required": ["startMs", "endMs", "text"],
                            "propertyOrdering": ["startMs", "endMs", "text"]
                        }
                    }
                }),
                required: vec!["segments".to_string()],
                property_ordering: vec!["segments".to_string()],
            },
        },
    }
}

pub fn extract_reply_text(response_body: &str) -> Result<String, AppError> {
    let response: GenerateContentResponse =
        serde_json::from_str(response_body).map_err(|error| AppError {
            code: "chat_response_parse_failed".to_string(),
            message: "Failed to parse Gemini chat response".to_string(),
            details: Some(error.to_string()),
        })?;

    let text = response
        .candidates
        .first()
        .and_then(|candidate| candidate.content.as_ref())
        .map(|content| {
            content
                .parts
                .iter()
                .filter_map(|part| part.text.as_deref())
                .collect::<String>()
        })
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| AppError {
            code: "empty_chat_response".to_string(),
            message: "Gemini chat response did not contain text".to_string(),
            details: None,
        })?;

    Ok(text)
}

pub fn extract_generated_images(response_body: &str) -> Result<Vec<GeneratedImage>, AppError> {
    let response: GenerateContentResponse =
        serde_json::from_str(response_body).map_err(|error| AppError {
            code: "image_response_parse_failed".to_string(),
            message: "Failed to parse Gemini image response".to_string(),
            details: Some(error.to_string()),
        })?;

    let images = response
        .candidates
        .iter()
        .filter_map(|candidate| candidate.content.as_ref())
        .flat_map(|content| {
            content
                .parts
                .iter()
                .filter_map(|part| part.inline_data.as_ref())
                .map(|image| GeneratedImage {
                    mime_type: image.mime_type.clone(),
                    data: image.data.clone(),
                })
        })
        .collect::<Vec<_>>();

    if images.is_empty() {
        return Err(AppError {
            code: "empty_image_response".to_string(),
            message: "Gemini image response did not contain image data".to_string(),
            details: None,
        });
    }

    Ok(images)
}

pub fn extract_subtitle_segments(response_body: &str) -> Result<Vec<SubtitleSegment>, AppError> {
    let response: GenerateContentResponse =
        serde_json::from_str(response_body).map_err(|error| AppError {
            code: "subtitle_response_parse_failed".to_string(),
            message: "Failed to parse Gemini subtitle response".to_string(),
            details: Some(error.to_string()),
        })?;

    let json_text = response
        .candidates
        .first()
        .and_then(|candidate| candidate.content.as_ref())
        .and_then(|content| content.parts.iter().find_map(|part| part.text.as_deref()))
        .ok_or_else(|| AppError {
            code: "empty_subtitle_response".to_string(),
            message: "Gemini subtitle response did not contain JSON text".to_string(),
            details: None,
        })?;

    let parsed: SubtitleSegmentsEnvelope =
        serde_json::from_str(json_text).map_err(|error| AppError {
            code: "subtitle_json_parse_failed".to_string(),
            message: "Failed to parse subtitle JSON".to_string(),
            details: Some(error.to_string()),
        })?;

    Ok(parsed.segments)
}

pub struct ReqwestGeminiTransport {
    api_key: String,
    timeout_ms: u64,
}

impl ReqwestGeminiTransport {
    pub fn new(api_key: String, timeout_ms: u64) -> Self {
        Self {
            api_key,
            timeout_ms,
        }
    }

    fn build_client(&self) -> Result<reqwest::blocking::Client, AppError> {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_millis(self.timeout_ms))
            .build()
            .map_err(|error| AppError {
                code: "gemini_request_setup_failed".to_string(),
                message: "Failed to prepare Gemini request".to_string(),
                details: Some(error.to_string()),
            })
    }

    fn post_generate_content<T: serde::Serialize>(
        &self,
        model: &str,
        payload: &T,
    ) -> Result<String, AppError> {
        let client = self.build_client()?;

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            model
        );

        let response = client
            .post(url)
            .header("x-goog-api-key", &self.api_key)
            .json(payload)
            .send()
            .map_err(|error| AppError {
                code: "gemini_request_failed".to_string(),
                message: "Failed to reach Gemini API".to_string(),
                details: Some(error.to_string()),
            })?;

        let status = response.status();
        let response_body = response.text().map_err(|error| AppError {
            code: "gemini_response_read_failed".to_string(),
            message: "Failed to read Gemini response".to_string(),
            details: Some(error.to_string()),
        })?;

        if !status.is_success() {
            return Err(AppError {
                code: "gemini_request_failed".to_string(),
                message: format!("Gemini request failed with HTTP {}", status),
                details: Some(sanitize_remote_error_details(&response_body)),
            });
        }

        Ok(response_body)
    }

    fn upload_file(
        &self,
        request: &GeminiSubtitleTransportRequest,
    ) -> Result<GeminiFileRecord, AppError> {
        let client = self.build_client()?;
        let start_response = client
            .post("https://generativelanguage.googleapis.com/upload/v1beta/files")
            .header("x-goog-api-key", &self.api_key)
            .header("X-Goog-Upload-Protocol", "resumable")
            .header("X-Goog-Upload-Command", "start")
            .header(
                "X-Goog-Upload-Header-Content-Length",
                request.data.len().to_string(),
            )
            .header("X-Goog-Upload-Header-Content-Type", &request.mime_type)
            .json(&serde_json::json!({
                "file": {
                    "displayName": request.file_name
                }
            }))
            .send()
            .map_err(|error| AppError {
                code: "file_upload_failed".to_string(),
                message: "Failed to start Gemini file upload".to_string(),
                details: Some(error.to_string()),
            })?;

        let upload_url = start_response
            .headers()
            .get("X-Goog-Upload-URL")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| AppError {
                code: "file_upload_failed".to_string(),
                message: "Gemini file upload URL was missing".to_string(),
                details: None,
            })?
            .to_string();

        let finalize_response = client
            .post(upload_url)
            .header("x-goog-api-key", &self.api_key)
            .header("X-Goog-Upload-Command", "upload, finalize")
            .header("X-Goog-Upload-Offset", "0")
            .body(request.data.clone())
            .send()
            .map_err(|error| AppError {
                code: "file_upload_failed".to_string(),
                message: "Failed to finalize Gemini file upload".to_string(),
                details: Some(error.to_string()),
            })?;

        let status = finalize_response.status();
        let body = finalize_response.text().map_err(|error| AppError {
            code: "file_upload_failed".to_string(),
            message: "Failed to read Gemini file upload response".to_string(),
            details: Some(error.to_string()),
        })?;

        if !status.is_success() {
            return Err(AppError {
                code: "file_upload_failed".to_string(),
                message: format!("Gemini file upload failed with HTTP {}", status),
                details: Some(sanitize_remote_error_details(&body)),
            });
        }

        let envelope: GeminiFileEnvelope =
            serde_json::from_str(&body).map_err(|error| AppError {
                code: "file_upload_failed".to_string(),
                message: "Failed to parse Gemini file upload response".to_string(),
                details: Some(error.to_string()),
            })?;

        Ok(envelope.file)
    }

    fn poll_file_active(&self, file_name: &str) -> Result<GeminiFileRecord, AppError> {
        let client = self.build_client()?;
        let url = format!("https://generativelanguage.googleapis.com/v1beta/{file_name}");

        for _ in 0..20 {
            let response = client
                .get(&url)
                .header("x-goog-api-key", &self.api_key)
                .send()
                .map_err(|error| AppError {
                    code: "file_poll_failed".to_string(),
                    message: "Failed to poll Gemini file status".to_string(),
                    details: Some(error.to_string()),
                })?;

            let status = response.status();
            let body = response.text().map_err(|error| AppError {
                code: "file_poll_failed".to_string(),
                message: "Failed to read Gemini file status".to_string(),
                details: Some(error.to_string()),
            })?;

            if !status.is_success() {
                return Err(AppError {
                    code: "file_poll_failed".to_string(),
                    message: format!("Gemini file polling failed with HTTP {}", status),
                    details: Some(sanitize_remote_error_details(&body)),
                });
            }

            let file: GeminiFileRecord = serde_json::from_str(&body).map_err(|error| AppError {
                code: "file_poll_failed".to_string(),
                message: "Failed to parse Gemini file status".to_string(),
                details: Some(error.to_string()),
            })?;

            if file.state == "ACTIVE" {
                return Ok(file);
            }

            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        Err(AppError {
            code: "file_poll_timeout".to_string(),
            message: "Timed out waiting for Gemini file to become active".to_string(),
            details: None,
        })
    }
}

impl GeminiTransport for ReqwestGeminiTransport {
    fn send_chat(
        &self,
        request: GeminiTransportRequest,
    ) -> Result<GeminiTransportResponse, AppError> {
        let payload = build_generate_content_request(&request);
        let response_body = self.post_generate_content(&request.model, &payload)?;
        let reply_text = extract_reply_text(&response_body)?;

        Ok(GeminiTransportResponse { reply_text })
    }
}

impl GeminiImageTransport for ReqwestGeminiTransport {
    fn send_image(
        &self,
        request: GeminiImageTransportRequest,
    ) -> Result<GeminiImageTransportResponse, AppError> {
        let payload = build_generate_image_request(&request);
        let response_body = self.post_generate_content(&request.model, &payload)?;
        let images = extract_generated_images(&response_body)?;

        Ok(GeminiImageTransportResponse { images })
    }
}

impl GeminiSubtitleTransport for ReqwestGeminiTransport {
    fn extract_subtitles(
        &self,
        request: GeminiSubtitleTransportRequest,
    ) -> Result<TranscriptResult, AppError> {
        let uploaded_file = self.upload_file(&request)?;
        let active_file = self.poll_file_active(&uploaded_file.name)?;
        let instruction = "Return JSON only with a top-level segments array. Each segment must include startMs, endMs, and text. Preserve the spoken language and keep timestamps in milliseconds.";
        let payload =
            build_generate_subtitle_request(instruction, &active_file.uri, &request.mime_type);
        let response_body = self.post_generate_content(&request.model, &payload)?;
        let segments = extract_subtitle_segments(&response_body)?;
        let artifact = write_srt_artifact(&segments, &request.export_dir, &request.file_name)?;

        Ok(TranscriptResult { segments, artifact })
    }
}

pub struct GeminiChatClient {
    transport: Box<dyn GeminiTransport>,
}

impl GeminiChatClient {
    pub fn new(transport: Box<dyn GeminiTransport>) -> Self {
        Self { transport }
    }

    pub fn production(api_key: String, timeout_ms: u64) -> Self {
        Self::new(Box::new(ReqwestGeminiTransport::new(api_key, timeout_ms)))
    }

    pub fn chat_generate(
        &self,
        model: String,
        prompt: String,
        history: Vec<ChatMessage>,
    ) -> Result<ChatResponse, AppError> {
        let transport_response = self.transport.send_chat(GeminiTransportRequest {
            model,
            prompt,
            history,
        })?;

        Ok(ChatResponse {
            message: ChatMessage {
                role: ChatRole::Assistant,
                content: transport_response.reply_text,
            },
        })
    }
}

pub struct GeminiImageClient {
    transport: Box<dyn GeminiImageTransport>,
}

impl GeminiImageClient {
    pub fn new(transport: Box<dyn GeminiImageTransport>) -> Self {
        Self { transport }
    }

    pub fn production(api_key: String, timeout_ms: u64) -> Self {
        Self::new(Box::new(ReqwestGeminiTransport::new(api_key, timeout_ms)))
    }

    pub fn generate_image(
        &self,
        request: ImageRequest,
    ) -> Result<ImageGenerationResponse, AppError> {
        let response = self.transport.send_image(GeminiImageTransportRequest {
            model: request.model,
            prompt: request.prompt,
            count: request.count,
            aspect_ratio: request.aspect_ratio,
        })?;

        Ok(ImageGenerationResponse {
            images: response.images,
        })
    }
}

pub struct GeminiSubtitleClient {
    transport: Box<dyn GeminiSubtitleTransport>,
}

impl GeminiSubtitleClient {
    pub fn new(transport: Box<dyn GeminiSubtitleTransport>) -> Self {
        Self { transport }
    }

    pub fn production(api_key: String, timeout_ms: u64) -> Self {
        Self::new(Box::new(ReqwestGeminiTransport::new(api_key, timeout_ms)))
    }

    pub fn extract_subtitles(
        &self,
        request: SubtitleExtractionRequest,
        export_dir: &str,
    ) -> Result<TranscriptResult, AppError> {
        self.transport
            .extract_subtitles(GeminiSubtitleTransportRequest {
                model: request.model,
                file_name: request.file_name,
                mime_type: request.mime_type,
                data: request.data,
                export_dir: export_dir.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_generate_content_request, build_generate_image_request,
        build_generate_subtitle_request, extract_generated_images, extract_reply_text,
        extract_subtitle_segments, GeminiImageTransportRequest, GeminiTransportRequest,
    };
    use crate::models::{ChatMessage, ChatRole};

    #[test]
    fn builds_generate_content_request_with_history_and_prompt() {
        let request = GeminiTransportRequest {
            model: "gemini-2.0-flash".to_string(),
            prompt: "最新问题".to_string(),
            history: vec![
                ChatMessage {
                    role: ChatRole::User,
                    content: "第一句".to_string(),
                },
                ChatMessage {
                    role: ChatRole::Assistant,
                    content: "第一轮回答".to_string(),
                },
            ],
        };

        let payload = build_generate_content_request(&request);
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["contents"][0]["role"], "user");
        assert_eq!(json["contents"][0]["parts"][0]["text"], "第一句");
        assert_eq!(json["contents"][1]["role"], "model");
        assert_eq!(json["contents"][1]["parts"][0]["text"], "第一轮回答");
        assert_eq!(json["contents"][2]["role"], "user");
        assert_eq!(json["contents"][2]["parts"][0]["text"], "最新问题");
    }

    #[test]
    fn extracts_reply_text_from_generate_content_response() {
        let response = r#"{
          "candidates": [
            {
              "content": {
                "role": "model",
                "parts": [
                  { "text": "hello " },
                  { "text": "from gemini" }
                ]
              }
            }
          ]
        }"#;

        let text = extract_reply_text(response).unwrap();
        assert_eq!(text, "hello from gemini");
    }

    #[test]
    fn builds_generate_image_request_with_generation_config() {
        let request = GeminiImageTransportRequest {
            model: "gemini-2.0-flash-preview-image-generation".to_string(),
            prompt: "一只红色的猫".to_string(),
            count: 1,
            aspect_ratio: "1:1".to_string(),
        };

        let payload = build_generate_image_request(&request);
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["contents"][0]["role"], "user");
        assert_eq!(json["contents"][0]["parts"][0]["text"], "一只红色的猫");
        assert_eq!(json["generationConfig"]["responseModalities"][0], "TEXT");
        assert_eq!(json["generationConfig"]["responseModalities"][1], "IMAGE");
        assert_eq!(
            json["generationConfig"]["responseFormat"]["image"]["aspectRatio"],
            "1:1"
        );
        assert_eq!(json["generationConfig"]["candidateCount"], 1);
    }

    #[test]
    fn extracts_inline_images_from_multiple_candidates_and_parts() {
        let response = r#"{
          "candidates": [
            {
              "content": {
                "role": "model",
                "parts": [
                  {
                    "text": "first candidate"
                  },
                  {
                    "inlineData": {
                      "mimeType": "image/png",
                      "data": "ZmFrZS1pbWFnZQ=="
                    }
                  },
                  {
                    "inlineData": {
                      "mimeType": "image/jpeg",
                      "data": "c2Vjb25kLWltYWdl"
                    }
                  }
                ]
              }
            },
            {
              "content": {
                "role": "model",
                "parts": [
                  {
                    "inlineData": {
                      "mimeType": "image/webp",
                      "data": "dGhpcmQtaW1hZ2U="
                    }
                  }
                ]
              }
            }
          ]
        }"#;

        let images = extract_generated_images(response).unwrap();
        assert_eq!(images.len(), 3);
        assert_eq!(images[0].mime_type, "image/png");
        assert_eq!(images[0].data, "ZmFrZS1pbWFnZQ==");
        assert_eq!(images[1].mime_type, "image/jpeg");
        assert_eq!(images[1].data, "c2Vjb25kLWltYWdl");
        assert_eq!(images[2].mime_type, "image/webp");
        assert_eq!(images[2].data, "dGhpcmQtaW1hZ2U=");
    }

    #[test]
    fn builds_generate_subtitle_request_with_file_data_and_json_schema() {
        let payload =
            build_generate_subtitle_request("Return subtitle JSON", "files/abc-123", "audio/wav");
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(
            json["contents"][0]["parts"][0]["text"],
            "Return subtitle JSON"
        );
        assert_eq!(
            json["contents"][0]["parts"][1]["fileData"]["mimeType"],
            "audio/wav"
        );
        assert_eq!(
            json["contents"][0]["parts"][1]["fileData"]["fileUri"],
            "files/abc-123"
        );
        assert_eq!(
            json["generationConfig"]["responseMimeType"],
            "application/json"
        );
        assert_eq!(
            json["generationConfig"]["responseSchema"]["propertyOrdering"][0],
            "segments"
        );
    }

    #[test]
    fn extracts_subtitle_segments_from_json_text_response() {
        let response = r#"{
          "candidates": [
            {
              "content": {
                "role": "model",
                "parts": [
                  {
                    "text": "{\"segments\":[{\"startMs\":0,\"endMs\":1500,\"text\":\"你好\"},{\"startMs\":1500,\"endMs\":3000,\"text\":\"世界\"}]}"
                  }
                ]
              }
            }
          ]
        }"#;

        let segments = extract_subtitle_segments(response).unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].start_ms, 0);
        assert_eq!(segments[0].text, "你好");
        assert_eq!(segments[1].end_ms, 3000);
        assert_eq!(segments[1].text, "世界");
    }
}
