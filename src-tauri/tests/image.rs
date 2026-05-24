use ai_dev_installer::error::AppError;
use ai_dev_installer::models::{GeneratedImage, ImageGenerationResponse};
use ai_dev_installer::services::image::{ImageRequest, ImageService};

struct FakeImageClient {
    response: ImageGenerationResponse,
}

impl FakeImageClient {
    fn new(response: ImageGenerationResponse) -> Self {
        Self { response }
    }
}

impl ai_dev_installer::services::image::GeminiImageClientLike for FakeImageClient {
    fn generate_image(&self, _request: ImageRequest) -> Result<ImageGenerationResponse, AppError> {
        Ok(self.response.clone())
    }
}

#[test]
fn rejects_empty_prompt_for_image_generation() {
    let service = ImageService::new(Box::new(FakeImageClient::new(ImageGenerationResponse {
        images: vec![],
    })));

    let error = service
        .generate(ImageRequest {
            model: "gemini-2.0-flash-preview-image-generation".to_string(),
            prompt: String::new(),
            count: 1,
            aspect_ratio: "1:1".to_string(),
        })
        .unwrap_err();

    assert_eq!(error.code, "invalid_prompt");
}

#[test]
fn normalizes_inline_image_response() {
    let service = ImageService::new(Box::new(FakeImageClient::new(ImageGenerationResponse {
        images: vec![GeneratedImage {
            mime_type: "image/png".to_string(),
            data: "ZmFrZS1pbWFnZQ==".to_string(),
        }],
    })));

    let response = service
        .generate(ImageRequest {
            model: "gemini-2.0-flash-preview-image-generation".to_string(),
            prompt: "一只红色的猫".to_string(),
            count: 1,
            aspect_ratio: "1:1".to_string(),
        })
        .unwrap();

    assert_eq!(response.images.len(), 1);
    assert_eq!(response.images[0].mime_type, "image/png");
    assert_eq!(response.images[0].data, "ZmFrZS1pbWFnZQ==");
}
