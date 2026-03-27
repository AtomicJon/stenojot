//! OpenAI Chat Completions API client implementation.
//!
//! Communicates with the OpenAI API at `POST /v1/chat/completions` using
//! non-streaming mode. Requires an API key in the `Authorization: Bearer` header.

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use super::provider::{LlmClient, LlmConfig, LlmError, LlmResponse};

/// Maximum tokens to request in the response.
const MAX_TOKENS: u32 = 4096;

/// HTTP client for the OpenAI Chat Completions API.
pub struct OpenAiClient {
    /// Base URL (e.g. `https://api.openai.com`).
    base_url: String,
    /// Model name to use.
    model: String,
    /// API key for authentication.
    api_key: String,
    /// Reusable HTTP client.
    client: Client,
}

impl OpenAiClient {
    /// Create a new OpenAI client from the given configuration.
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

/// Request body for `POST /v1/chat/completions`.
#[derive(Serialize)]
struct CompletionsRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ChatMessage>,
}

/// A single message in the OpenAI chat format.
#[derive(Serialize, Deserialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

/// Successful response from the Chat Completions API.
#[derive(Deserialize)]
struct CompletionsResponse {
    choices: Option<Vec<Choice>>,
    error: Option<ApiError>,
}

/// A single choice in the completions response.
#[derive(Deserialize)]
struct Choice {
    message: Option<ChatMessage>,
}

/// Error object from the API.
#[derive(Deserialize)]
struct ApiError {
    message: String,
}

/// Build the JSON request body for the OpenAI Chat Completions API.
pub fn build_request_body(
    model: &str,
    system_prompt: &str,
    user_prompt: &str,
) -> serde_json::Value {
    serde_json::to_value(CompletionsRequest {
        model: model.to_string(),
        max_tokens: MAX_TOKENS,
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            },
        ],
    })
    .expect("Failed to serialize OpenAI request")
}

/// Parse the OpenAI Chat Completions API response body into an [`LlmResponse`].
pub fn parse_completions_response(body: &str) -> Result<LlmResponse, LlmError> {
    let resp: CompletionsResponse =
        serde_json::from_str(body).map_err(|e| LlmError::ParseError(e.to_string()))?;

    if let Some(error) = resp.error {
        return Err(LlmError::ApiError {
            status: 0,
            message: error.message,
        });
    }

    let text = resp
        .choices
        .unwrap_or_default()
        .into_iter()
        .filter_map(|c| c.message)
        .map(|m| m.content)
        .next()
        .unwrap_or_default();

    if text.is_empty() {
        return Err(LlmError::ParseError(
            "OpenAI response contained no message content".to_string(),
        ));
    }

    Ok(LlmResponse { text })
}

impl LlmClient for OpenAiClient {
    fn complete(&self, system_prompt: &str, user_prompt: &str) -> Result<LlmResponse, LlmError> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        let request_body = build_request_body(&self.model, system_prompt, user_prompt);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .map_err(|e: reqwest::Error| LlmError::Network(e.to_string()))?;

        let status = response.status().as_u16();
        let body = response
            .text()
            .map_err(|e: reqwest::Error| LlmError::Network(e.to_string()))?;

        if status >= 400 {
            if let Ok(resp) = serde_json::from_str::<CompletionsResponse>(&body) {
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

        parse_completions_response(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_completions_response_success() {
        // Arrange
        let body = r###"{
            "id": "chatcmpl-123",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "## Key Points - Item 1"},
                "finish_reason": "stop"
            }]
        }"###;

        // Act
        let result = parse_completions_response(body);

        // Assert
        let resp = result.unwrap();
        assert_eq!(resp.text, "## Key Points - Item 1");
    }

    #[test]
    fn parse_completions_response_with_error() {
        // Arrange
        let body = r#"{
            "error": {"message": "Incorrect API key", "type": "invalid_request_error"}
        }"#;

        // Act
        let result = parse_completions_response(body);

        // Assert
        assert!(matches!(result, Err(LlmError::ApiError { .. })));
        if let Err(LlmError::ApiError { message, .. }) = result {
            assert!(message.contains("Incorrect API key"));
        }
    }

    #[test]
    fn parse_completions_response_empty_choices() {
        // Arrange
        let body = r#"{"choices": []}"#;

        // Act
        let result = parse_completions_response(body);

        // Assert
        assert!(matches!(result, Err(LlmError::ParseError(_))));
    }

    #[test]
    fn parse_completions_response_invalid_json() {
        // Arrange
        let body = "not json";

        // Act
        let result = parse_completions_response(body);

        // Assert
        assert!(matches!(result, Err(LlmError::ParseError(_))));
    }

    #[test]
    fn build_request_body_has_correct_structure() {
        // Arrange
        let model = "gpt-4o";
        let system = "You are helpful.";
        let user = "Summarize this.";

        // Act
        let body = build_request_body(model, system, user);

        // Assert
        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["max_tokens"], MAX_TOKENS);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "You are helpful.");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "Summarize this.");
    }

    #[test]
    fn openai_client_new_uses_config() {
        // Arrange
        let config = LlmConfig {
            provider: super::super::provider::LlmProvider::OpenAi,
            model: "gpt-4-turbo".to_string(),
            api_key: Some("sk-test".to_string()),
            base_url: None,
        };

        // Act
        let client = OpenAiClient::new(&config);

        // Assert
        assert_eq!(client.base_url, "https://api.openai.com");
        assert_eq!(client.model, "gpt-4-turbo");
        assert_eq!(client.api_key, "sk-test");
    }

    #[test]
    fn openai_client_new_custom_base_url() {
        // Arrange
        let config = LlmConfig {
            provider: super::super::provider::LlmProvider::OpenAi,
            model: String::new(),
            api_key: Some("key".to_string()),
            base_url: Some("https://my-proxy.example.com".to_string()),
        };

        // Act
        let client = OpenAiClient::new(&config);

        // Assert
        assert_eq!(client.base_url, "https://my-proxy.example.com");
        assert_eq!(client.model, "gpt-4o");
    }
}
