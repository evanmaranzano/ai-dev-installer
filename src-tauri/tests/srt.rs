use ai_dev_installer::models::SubtitleSegment;
use ai_dev_installer::services::srt::render_srt;

#[test]
fn renders_srt_with_correct_timestamp_format() {
    let segments = vec![
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
    ];

    let srt = render_srt(&segments);

    assert!(srt.contains("00:00:00,000 --> 00:00:01,500"));
    assert!(srt.contains("00:00:01,500 --> 00:00:03,000"));
}
