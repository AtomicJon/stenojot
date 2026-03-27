//! Anthropic Messages API client implementation.
//!
//! Communicates with the Anthropic API at `POST /v1/messages` using
//! non-streaming mode. Requires an API key set in the `x-api-key` header.

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use super::provider::{LlmClient, LlmConfig, LlmError, LlmResponse};

/// Anthropic API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Maximum tokens to request in the response.
const MAX_TOKENS: u32 = 4096;

/// HTTP client for the Anthropic Messages API.
pub struct AnthropicClient {
    /// Base URL (e.g. `https://api.anthropic.com`).
    base_url: String,
    /// Model name to use.
    model: String,
    /// API key for authentication.
    api_key: String,
    /// Reusable HTTP client.
    client: Client,
}

impl AnthropicClient {
    /// Create a new Anthropic client from the given configuration.
    pub fn new(config: &LlmConfig) -> Self {
        Self {
            base_url: config
                .effective_base_url()
                .trim_end_matches('/')
                .to_string(),
            model: config.effective_model().to_string(),
            api_key: config.api_key.clone().unwrap_or_default(),
            client: Client::new(),
        }
    }
}

/// Request body for `POST /v1/messages`.
#[derive(Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
}

/// A single message in the Anthropic chat format.
#[derive(Serialize, Deserialize, Clone)]
struct Message {
    role: String,
    content: String,
}

/// Successful response from the Messages API.
#[derive(Deserialize)]
struct MessagesResponse {
    content: Option<Vec<ContentBlock>>,
    error: Option<ApiError>,
}

/// A content block in the response.
#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

/// Error object from the API.
#[derive(Deserialize)]
struct ApiError {
    message: String,
}

/// Build the JSON request body for the Anthropic Messages API.
pub fn build_request_body(
    model: &str,
    system_prompt: &str,
    user_prompt: &str,
) -> serde_json::Value {
    serde_json::to_value(MessagesRequest {
        model: model.to_string(),
        max_tokens: MAX_TOKENS,
        system: system_prompt.to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: user_prompt.to_string(),
        }],
    })
    .expect("Failed to serialize Anthropic request")
}

/// Parse the Anthropic Messages API response body into an [`LlmResponse`].
pub fn parse_messages_response(body: &str) -> Result<LlmResponse, LlmError> {
    let resp: MessagesResponse =
        serde_json::from_str(body).map_err(|e| LlmError::ParseError(e.to_string()))?;

    if let Some(error) = resp.error {
        return Err(LlmError::ApiError {
            status: 0,
            message: error.message,
        });
    }

    let text = resp
        .content
        .unwrap_or_default()
        .into_iter()
        .filter(|b| b.block_type == "text")
        .filter_map(|b| b.text)
        .collect::<Vec<_>>()
        .join("");

    if text.is_empty() {
        return Err(LlmError::ParseError(
            "Anthropic response contained no text content".to_string(),
        ));
    }

    Ok(LlmResponse { text })
}

impl LlmClient for AnthropicClient {
    fn complete(&self, system_prompt: &str, user_prompt: &str) -> Result<LlmResponse, LlmError> {
        let url = format!("{}/v1/messages", self.base_url);

        let request_body = build_request_body(&self.model, system_prompt, user_prompt);

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .map_err(|e: reqwest::Error| LlmError::Network(e.to_string()))?;

        let status = response.status().as_u16();
        let body = response
            .text()
            .map_err(|e: reqwest::Error| LlmError::Network(e.to_string()))?;

        if status >= 400 {
            // Try to parse the error from the response body
            if let Ok(resp) = serde_json::from_str::<MessagesResponse>(&body) {
                if let Some(error) = resp.error {
                    return Err(LlmError::ApiError {
                        status,
                        message: error.message,
                    });
                }
            }
            return Err(LlmError::ApiError {
                status,
                message: body,
            });
        }

        parse_messages_response(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_messages_response_success() {
        // Arrange
        let body = r###"{
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "## Key Points - Item 1"}],
            "stop_reason": "end_turn"
        }"###;

        // Act
        let result = parse_messages_response(body);

        // Assert
        let resp = result.unwrap();
        assert_eq!(resp.text, "## Key Points - Item 1");
    }

    #[test]
    fn parse_messages_response_multiple_content_blocks() {
        // Arrange
        let body = r#"{
            "content": [
                {"type": "text", "text": "Part 1"},
                {"type": "text", "text": " Part 2"}
            ]
        }"#;

        // Act
        let result = parse_messages_response(body);

        // Assert
        let resp = result.unwrap();
        assert_eq!(resp.text, "Part 1 Part 2");
    }

    #[test]
    fn parse_messages_response_with_error() {
        // Arrange
        let body = r#"{
            "type": "error",
            "error": {"type": "invalid_request_error", "message": "Invalid API key"}
        }"#;

        // Act
        let result = parse_messages_response(body);

        // Assert
        assert!(matches!(result, Err(LlmError::ApiError { .. })));
        if let Err(LlmError::ApiError { message, .. }) = result {
            assert!(message.contains("Invalid API key"));
        }
    }

    #[test]
    fn parse_messages_response_empty_content() {
        // Arrange
        let body = r#"{"content": []}"#;

        // Act
        let result = parse_messages_response(body);

        // Assert
        assert!(matches!(result, Err(LlmError::ParseError(_))));
    }

    #[test]
    fn parse_messages_response_invalid_json() {
        // Arrange
        let body = "not json";

        // Act
        let result = parse_messages_response(body);

        // Assert
        assert!(matches!(result, Err(LlmError::ParseError(_))));
    }

    #[test]
    fn build_request_body_has_correct_structure() {
        // Arrange
        let model = "claude-sonnet-4-20250514";
        let system = "You are helpful.";
        let user = "Summarize this.";

        // Act
        let body = build_request_body(model, system, user);

        // Assert
        assert_eq!(body["model"], "claude-sonnet-4-20250514");
        assert_eq!(body["max_tokens"], MAX_TOKENS);
        assert_eq!(body["system"], "You are helpful.");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "Summarize this.");
    }

    #[test]
    fn anthropic_client_new_uses_config() {
        // Arrange
        let config = LlmConfig {
            provider: super::super::provider::LlmProvider::Anthropic,
            model: "claude-opus-4-20250514".to_string(),
            api_key: Some("sk-ant-test".to_string()),
            base_url: None,
        };

        // Act
        let client = AnthropicClient::new(&config);

        // Assert
        assert_eq!(client.base_url, "https://api.anthropic.com");
        assert_eq!(client.model, "claude-opus-4-20250514");
        assert_eq!(client.api_key, "sk-ant-test");
    }
}
