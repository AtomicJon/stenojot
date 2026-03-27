//! LLM provider abstraction with trait-based client interface.
//!
//! Defines the core [`LlmClient`] trait that all provider implementations
//! must satisfy, along with configuration types and a factory function
//! for creating the appropriate client based on user settings.

use serde::{Deserialize, Serialize};
use std::fmt;

use super::anthropic::AnthropicClient;
use super::ollama::OllamaClient;
use super::openai::OpenAiClient;

/// Default Ollama model.
pub const DEFAULT_OLLAMA_MODEL: &str = "llama3.1";

/// Default Anthropic model.
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-20250514";

/// Default OpenAI model.
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o";

/// Default Ollama base URL.
pub const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

/// Default Anthropic API URL.
pub const DEFAULT_ANTHROPIC_URL: &str = "https://api.anthropic.com";

/// Default OpenAI API URL.
pub const DEFAULT_OPENAI_URL: &str = "https://api.openai.com";

/// LLM provider identifier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    /// Local LLM via Ollama HTTP API.
    Ollama,
    /// Anthropic Messages API (Claude).
    Anthropic,
    /// OpenAI Chat Completions API.
    #[serde(rename = "openai")]
    OpenAi,
}

impl fmt::Display for LlmProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmProvider::Ollama => write!(f, "ollama"),
            LlmProvider::Anthropic => write!(f, "anthropic"),
            LlmProvider::OpenAi => write!(f, "openai"),
        }
    }
}

/// Parse a provider string into an [`LlmProvider`], defaulting to Ollama.
pub fn parse_provider(s: &str) -> LlmProvider {
    match s.to_lowercase().as_str() {
        "anthropic" => LlmProvider::Anthropic,
        "openai" => LlmProvider::OpenAi,
        _ => LlmProvider::Ollama,
    }
}

/// Configuration for an LLM provider.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// Which provider to use.
    pub provider: LlmProvider,
    /// Model name (empty string means use provider default).
    pub model: String,
    /// API key for cloud providers (not needed for Ollama).
    pub api_key: Option<String>,
    /// Custom base URL override.
    pub base_url: Option<String>,
}

impl LlmConfig {
    /// Resolve the effective model name, falling back to the provider default.
    pub fn effective_model(&self) -> &str {
        if self.model.is_empty() {
            match self.provider {
                LlmProvider::Ollama => DEFAULT_OLLAMA_MODEL,
                LlmProvider::Anthropic => DEFAULT_ANTHROPIC_MODEL,
                LlmProvider::OpenAi => DEFAULT_OPENAI_MODEL,
            }
        } else {
            &self.model
        }
    }

    /// Resolve the effective base URL, falling back to the provider default.
    pub fn effective_base_url(&self) -> &str {
        if let Some(ref url) = self.base_url {
            if !url.is_empty() {
                return url;
            }
        }
        match self.provider {
            LlmProvider::Ollama => DEFAULT_OLLAMA_URL,
            LlmProvider::Anthropic => DEFAULT_ANTHROPIC_URL,
            LlmProvider::OpenAi => DEFAULT_OPENAI_URL,
        }
    }
}

/// Successful response from an LLM completion call.
#[derive(Debug, Clone)]
pub struct LlmResponse {
    /// The generated text content.
    pub text: String,
}

/// Errors that can occur during LLM calls.
#[derive(Debug)]
pub enum LlmError {
    /// Network or HTTP transport error.
    Network(String),
    /// Provider returned an error response (rate limit, invalid key, etc.).
    ApiError {
        /// HTTP status code.
        status: u16,
        /// Error message from the provider.
        message: String,
    },
    /// Response could not be parsed.
    ParseError(String),
    /// Provider not available (e.g. Ollama not running).
    Unavailable(String),
    /// Missing required configuration (e.g. API key for cloud provider).
    MissingConfig(String),
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmError::Network(msg) => write!(f, "Network error: {}", msg),
            LlmError::ApiError { status, message } => {
                write!(f, "API error ({}): {}", status, message)
            }
            LlmError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            LlmError::Unavailable(msg) => write!(f, "Provider unavailable: {}", msg),
            LlmError::MissingConfig(msg) => write!(f, "Missing config: {}", msg),
        }
    }
}

/// Trait for LLM provider implementations.
///
/// Each provider (Ollama, Anthropic, OpenAI) implements this trait to
/// provide a uniform interface for sending completion requests.
/// Implementations use `reqwest::blocking` for HTTP calls since summary
/// generation runs on a background `std::thread`.
pub trait LlmClient: Send + Sync {
    /// Send a completion request with system and user prompts.
    fn complete(&self, system_prompt: &str, user_prompt: &str) -> Result<LlmResponse, LlmError>;
}

/// Create the appropriate LLM client for the given configuration.
///
/// Validates that required configuration (e.g. API keys for cloud providers)
/// is present before constructing the client.
pub fn create_client(config: &LlmConfig) -> Result<Box<dyn LlmClient>, LlmError> {
    match config.provider {
        LlmProvider::Ollama => Ok(Box::new(OllamaClient::new(config))),
        LlmProvider::Anthropic => {
            if config.api_key.as_deref().unwrap_or("").is_empty() {
                return Err(LlmError::MissingConfig(
                    "Anthropic API key is required. Set it in Settings → AI Summary.".to_string(),
                ));
            }
            Ok(Box::new(AnthropicClient::new(config)))
        }
        LlmProvider::OpenAi => {
            if config.api_key.as_deref().unwrap_or("").is_empty() {
                return Err(LlmError::MissingConfig(
                    "OpenAI API key is required. Set it in Settings → AI Summary.".to_string(),
                ));
            }
            Ok(Box::new(OpenAiClient::new(config)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_provider_returns_ollama_for_unknown() {
        // Arrange
        let input = "unknown_provider";

        // Act
        let result = parse_provider(input);

        // Assert
        assert_eq!(result, LlmProvider::Ollama);
    }

    #[test]
    fn parse_provider_case_insensitive() {
        // Arrange / Act / Assert
        assert_eq!(parse_provider("Anthropic"), LlmProvider::Anthropic);
        assert_eq!(parse_provider("OPENAI"), LlmProvider::OpenAi);
        assert_eq!(parse_provider("OLLAMA"), LlmProvider::Ollama);
    }

    #[test]
    fn config_effective_model_uses_default_when_empty() {
        // Arrange
        let config = LlmConfig {
            provider: LlmProvider::Ollama,
            model: String::new(),
            api_key: None,
            base_url: None,
        };

        // Act
        let model = config.effective_model();

        // Assert
        assert_eq!(model, DEFAULT_OLLAMA_MODEL);
    }

    #[test]
    fn config_effective_model_uses_custom_when_set() {
        // Arrange
        let config = LlmConfig {
            provider: LlmProvider::Anthropic,
            model: "claude-opus-4-20250514".to_string(),
            api_key: Some("key".to_string()),
            base_url: None,
        };

        // Act
        let model = config.effective_model();

        // Assert
        assert_eq!(model, "claude-opus-4-20250514");
    }

    #[test]
    fn config_effective_base_url_uses_default_when_none() {
        // Arrange
        let config = LlmConfig {
            provider: LlmProvider::OpenAi,
            model: String::new(),
            api_key: None,
            base_url: None,
        };

        // Act
        let url = config.effective_base_url();

        // Assert
        assert_eq!(url, DEFAULT_OPENAI_URL);
    }

    #[test]
    fn config_effective_base_url_uses_custom_when_set() {
        // Arrange
        let config = LlmConfig {
            provider: LlmProvider::Ollama,
            model: String::new(),
            api_key: None,
            base_url: Some("http://myollama:11434".to_string()),
        };

        // Act
        let url = config.effective_base_url();

        // Assert
        assert_eq!(url, "http://myollama:11434");
    }

    #[test]
    fn create_client_ollama_succeeds_without_api_key() {
        // Arrange
        let config = LlmConfig {
            provider: LlmProvider::Ollama,
            model: String::new(),
            api_key: None,
            base_url: None,
        };

        // Act
        let result = create_client(&config);

        // Assert
        assert!(result.is_ok());
    }

    #[test]
    fn create_client_anthropic_requires_api_key() {
        // Arrange
        let config = LlmConfig {
            provider: LlmProvider::Anthropic,
            model: String::new(),
            api_key: None,
            base_url: None,
        };

        // Act
        let result = create_client(&config);

        // Assert
        assert!(matches!(result, Err(LlmError::MissingConfig(_))));
    }

    #[test]
    fn create_client_openai_requires_api_key() {
        // Arrange
        let config = LlmConfig {
            provider: LlmProvider::OpenAi,
            model: String::new(),
            api_key: None,
            base_url: None,
        };

        // Act
        let result = create_client(&config);

        // Assert
        assert!(matches!(result, Err(LlmError::MissingConfig(_))));
    }

    #[test]
    fn create_client_anthropic_succeeds_with_api_key() {
        // Arrange
        let config = LlmConfig {
            provider: LlmProvider::Anthropic,
            model: String::new(),
            api_key: Some("sk-test-key".to_string()),
            base_url: None,
        };

        // Act
        let result = create_client(&config);

        // Assert
        assert!(result.is_ok());
    }

    #[test]
    fn llm_provider_display() {
        // Arrange / Act / Assert
        assert_eq!(LlmProvider::Ollama.to_string(), "ollama");
        assert_eq!(LlmProvider::Anthropic.to_string(), "anthropic");
        assert_eq!(LlmProvider::OpenAi.to_string(), "openai");
    }

    #[test]
    fn llm_error_display() {
        // Arrange
        let errors = vec![
            (
                LlmError::Network("timeout".to_string()),
                "Network error: timeout",
            ),
            (
                LlmError::ApiError {
                    status: 429,
                    message: "rate limit".to_string(),
                },
                "API error (429): rate limit",
            ),
            (
                LlmError::ParseError("bad json".to_string()),
                "Parse error: bad json",
            ),
            (
                LlmError::Unavailable("not running".to_string()),
                "Provider unavailable: not running",
            ),
            (
                LlmError::MissingConfig("no key".to_string()),
                "Missing config: no key",
            ),
        ];

        for (error, expected) in errors {
            // Act
            let display = error.to_string();

            // Assert
            assert_eq!(display, expected);
        }
    }
}
