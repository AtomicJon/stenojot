//! Ollama LLM client implementation.
//!
//! Communicates with a local Ollama instance via its HTTP API at
//! `POST /api/chat`. Ollama returns newline-delimited JSON objects;
//! this client reads the full (non-streaming) response.

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use super::provider::{LlmClient, LlmConfig, LlmError, LlmResponse};

/// HTTP client for the Ollama `/api/chat` endpoint.
pub struct OllamaClient {
    /// Base URL (e.g. `http://localhost:11434`).
    base_url: String,
    /// Model name to use.
    model: String,
    /// Reusable HTTP client.
    client: Client,
}

impl OllamaClient {
    /// Create a new Ollama client from the given configuration.
    pub fn new(config: &LlmConfig) -> Self {
        Self {
            base_url: config.effective_base_url().trim_end_matches('/').to_string(),
            model: config.effective_model().to_string(),
            client: Client::new(),
        }
    }
}

/// Request body for `POST /api/chat`.
#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
}

/// A single message in the Ollama chat format.
#[derive(Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

/// Response body from `POST /api/chat` (non-streaming).
#[derive(Deserialize)]
struct ChatResponse {
    message: Option<ChatMessage>,
    error: Option<String>,
}

/// Parse the Ollama chat response body into an [`LlmResponse`].
pub fn parse_chat_response(body: &str) -> Result<LlmResponse, LlmError> {
    let resp: ChatResponse =
        serde_json::from_str(body).map_err(|e| LlmError::ParseError(e.to_string()))?;

    if let Some(error) = resp.error {
        return Err(LlmError::ApiError {
            status: 0,
            message: error,
        });
    }

    let text = resp
        .message
        .map(|m| m.content)
        .unwrap_or_default();

    if text.is_empty() {
        return Err(LlmError::ParseError(
            "Ollama response contained no message content".to_string(),
        ));
    }

    Ok(LlmResponse { text })
}

impl LlmClient for OllamaClient {
    fn complete(&self, system_prompt: &str, user_prompt: &str) -> Result<LlmResponse, LlmError> {
        let url = format!("{}/api/chat", self.base_url);

        let request = ChatRequest {
            model: self.model.clone(),
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
            stream: false,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .map_err(|e: reqwest::Error| {
                if e.is_connect() {
                    LlmError::Unavailable(format!(
                        "Cannot connect to Ollama at {}. Is Ollama running?",
                        self.base_url
                    ))
                } else {
                    LlmError::Network(e.to_string())
                }
            })?;

        let status = response.status().as_u16();
        let body = response.text().map_err(|e: reqwest::Error| LlmError::Network(e.to_string()))?;

        if status >= 400 {
            return Err(LlmError::ApiError {
                status,
                message: body,
            });
        }

        parse_chat_response(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_chat_response_success() {
        // Arrange
        let body = r#"{"message":{"role":"assistant","content":"Hello world"},"done":true}"#;

        // Act
        let result = parse_chat_response(body);

        // Assert
        let resp = result.unwrap();
        assert_eq!(resp.text, "Hello world");
    }

    #[test]
    fn parse_chat_response_with_error() {
        // Arrange
        let body = r#"{"error":"model not found"}"#;

        // Act
        let result = parse_chat_response(body);

        // Assert
        assert!(matches!(result, Err(LlmError::ApiError { status: 0, .. })));
        if let Err(LlmError::ApiError { message, .. }) = result {
            assert!(message.contains("model not found"));
        }
    }

    #[test]
    fn parse_chat_response_empty_message() {
        // Arrange
        let body = r#"{"message":{"role":"assistant","content":""}}"#;

        // Act
        let result = parse_chat_response(body);

        // Assert
        assert!(matches!(result, Err(LlmError::ParseError(_))));
    }

    #[test]
    fn parse_chat_response_invalid_json() {
        // Arrange
        let body = "not json at all";

        // Act
        let result = parse_chat_response(body);

        // Assert
        assert!(matches!(result, Err(LlmError::ParseError(_))));
    }

    #[test]
    fn parse_chat_response_missing_message_field() {
        // Arrange
        let body = r#"{"done":true}"#;

        // Act
        let result = parse_chat_response(body);

        // Assert
        assert!(matches!(result, Err(LlmError::ParseError(_))));
    }

    #[test]
    fn ollama_client_new_uses_defaults() {
        // Arrange
        let config = LlmConfig {
            provider: super::super::provider::LlmProvider::Ollama,
            model: String::new(),
            api_key: None,
            base_url: None,
        };

        // Act
        let client = OllamaClient::new(&config);

        // Assert
        assert_eq!(client.base_url, "http://localhost:11434");
        assert_eq!(client.model, "llama3.1");
    }

    #[test]
    fn ollama_client_new_uses_custom_values() {
        // Arrange
        let config = LlmConfig {
            provider: super::super::provider::LlmProvider::Ollama,
            model: "mistral".to_string(),
            api_key: None,
            base_url: Some("http://myserver:8080/".to_string()),
        };

        // Act
        let client = OllamaClient::new(&config);

        // Assert
        assert_eq!(client.base_url, "http://myserver:8080");
        assert_eq!(client.model, "mistral");
    }
}
