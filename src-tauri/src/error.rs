use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
}

const MAX_REMOTE_ERROR_DETAILS_CHARS: usize = 2048;

pub(crate) fn sanitize_remote_error_details(body: &str) -> String {
    let summary = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| summarize_json_error(&value))
        .unwrap_or_else(|| body.to_string());

    truncate_chars(&redact_sensitive_fields(&summary), MAX_REMOTE_ERROR_DETAILS_CHARS)
}

fn summarize_json_error(value: &serde_json::Value) -> Option<String> {
    let error = value.get("error").unwrap_or(value);
    let code = error
        .get("code")
        .and_then(|value| {
            value
                .as_i64()
                .map(|number| number.to_string())
                .or_else(|| value.as_str().map(str::to_string))
        });
    let status = error
        .get("status")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let message = error
        .get("message")
        .and_then(|value| value.as_str())
        .map(str::to_string);

    let mut parts = Vec::new();
    if let Some(code) = code {
        parts.push(format!("code={code}"));
    }
    if let Some(status) = status {
        parts.push(format!("status={status}"));
    }
    if let Some(message) = message {
        parts.push(format!("message={message}"));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

fn redact_sensitive_fields(input: &str) -> String {
    let mut output = Vec::new();
    let tokens: Vec<&str> = input.split_whitespace().collect();
    let mut index = 0;

    while index < tokens.len() {
        let token = tokens[index];
        if let Some(kind) = sensitive_token_kind(token) {
            output.push(redact_token(token));
            index = next_non_sensitive_token_index(&tokens, index, kind);
        } else {
            output.push(token.to_string());
            index += 1;
        }
    }

    output.join(" ")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SensitiveTokenKind {
    Authorization,
    Other,
}

fn sensitive_token_kind(token: &str) -> Option<SensitiveTokenKind> {
    let lower = token.to_ascii_lowercase();
    let key = sensitive_key_in_token(&lower)?;

    if key == "authorization" {
        return Some(SensitiveTokenKind::Authorization);
    }

    if [
        "apikey",
        "api_key",
        "x-goog-api-key",
        "fileuri",
        "file_uri",
        "uri",
    ]
    .iter()
    .any(|marker| key == *marker)
    {
        Some(SensitiveTokenKind::Other)
    } else {
        None
    }
}

fn sensitive_key_in_token(token: &str) -> Option<&str> {
    [
        "authorization",
        "x-goog-api-key",
        "api_key",
        "apikey",
        "file_uri",
        "fileuri",
        "uri",
    ]
    .into_iter()
    .find(|key| token_contains_sensitive_key(token, key))
}

fn token_contains_sensitive_key(token: &str, key: &str) -> bool {
    let mut search_start = 0;
    while let Some(relative_index) = token[search_start..].find(key) {
        let start = search_start + relative_index;
        let end = start + key.len();
        let before = token[..start].chars().next_back();
        let after = token[end..].chars().next();

        let before_ok = before
            .map(|character| !character.is_ascii_alphanumeric() && character != '-' && character != '_')
            .unwrap_or(true);
        let after_ok = after
            .map(|character| {
                !character.is_ascii_alphanumeric() && character != '-' && character != '_'
            })
            .unwrap_or(true);

        if before_ok && after_ok {
            return true;
        }

        search_start = end;
    }

    false
}

fn redact_token(token: &str) -> String {
    let separator = sensitive_value_separator_index(token);
    if let Some(index) = separator {
        format!("{}[redacted]", &token[..=index])
    } else {
        "[redacted]".to_string()
    }
}

fn sensitive_value_separator_index(token: &str) -> Option<usize> {
    let lower = token.to_ascii_lowercase();
    let key = sensitive_key_in_token(&lower)?;
    let key_start = lower.find(key)?;

    token[key_start + key.len()..]
        .find(['=', ':'])
        .map(|offset| key_start + key.len() + offset)
}

fn next_non_sensitive_token_index(
    tokens: &[&str],
    current: usize,
    kind: SensitiveTokenKind,
) -> usize {
    let token = tokens[current];
    let lower = token.to_ascii_lowercase();
    let mut next = current + 1;
    let has_inline_value = sensitive_value_separator_index(token)
        .map(|index| index + 1 < token.len())
        .unwrap_or(false);

    if next < tokens.len() && is_separator_token(tokens[next]) {
        next += 1;
    }

    if kind == SensitiveTokenKind::Authorization {
        let has_inline_bearer = lower.contains(":bearer") || lower.contains("=bearer");

        if has_inline_bearer {
            next += 1;
        } else {
            let consumed_bearer = if next < tokens.len() && tokens[next].eq_ignore_ascii_case("bearer") {
                next += 1;
                true
            } else {
                false
            };

            if next < tokens.len() && (!has_inline_value || consumed_bearer) {
                next += 1;
            }
        }
    } else if !has_inline_value && next < tokens.len() {
        next += 1;
    }

    next
}

fn is_separator_token(token: &str) -> bool {
    matches!(token, "=" | ":")
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for (index, character) in input.chars().enumerate() {
        if index >= max_chars {
            output.push_str("...");
            return output;
        }
        output.push(character);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::sanitize_remote_error_details;

    #[test]
    fn remote_error_details_use_google_error_summary_and_redaction() {
        let details = sanitize_remote_error_details(
            r#"{"error":{"code":403,"status":"PERMISSION_DENIED","message":"bad apiKey and fileUri"}}"#,
        );

        assert!(details.contains("code=403"));
        assert!(details.contains("status=PERMISSION_DENIED"));
        assert!(!details.contains("apiKey"));
        assert!(!details.contains("fileUri"));
    }

    #[test]
    fn remote_error_details_redact_sensitive_values() {
        let details = sanitize_remote_error_details(
            r#"{"error":{"message":"Authorization: Bearer secret-token x-goog-api-key=secret-key file_uri=https://example.invalid/file"}}"#,
        );

        assert!(!details.contains("secret-token"));
        assert!(!details.contains("secret-key"));
        assert!(!details.contains("https://example.invalid/file"));
        assert!(details.contains("[redacted]"));
    }

    #[test]
    fn remote_error_details_redact_compact_authorization_values() {
        let details = sanitize_remote_error_details(
            r#"{"error":{"message":"Authorization:Bearer compact-secret Authorization=Bearer equals-secret apiKey:compact-key fileUri:https://example.invalid/file"}}"#,
        );

        assert!(!details.contains("compact-secret"));
        assert!(!details.contains("equals-secret"));
        assert!(!details.contains("compact-key"));
        assert!(!details.contains("https://example.invalid/file"));
        assert!(details.contains("[redacted]"));
    }

    #[test]
    fn remote_error_details_redact_separated_sensitive_values() {
        let details = sanitize_remote_error_details(
            r#"{"error":{"message":"apiKey separated-key apiKey = spaced-key fileUri https://example.invalid/file file_uri = https://example.invalid/other uri https://example.invalid/uri"}}"#,
        );

        assert!(!details.contains("separated-key"));
        assert!(!details.contains("spaced-key"));
        assert!(!details.contains("https://example.invalid/file"));
        assert!(!details.contains("https://example.invalid/other"));
        assert!(!details.contains("https://example.invalid/uri"));
        assert!(details.contains("[redacted]"));
    }

    #[test]
    fn remote_error_details_are_length_limited() {
        let details = sanitize_remote_error_details(&"a".repeat(3000));
        assert!(details.len() <= 2051);
        assert!(details.ends_with("..."));
    }
}
