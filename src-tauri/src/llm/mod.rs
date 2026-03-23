//! LLM integration for AI-powered meeting summaries and title generation.
//!
//! Provides a trait-based abstraction over multiple LLM providers (Ollama,
//! Anthropic, OpenAI) with chunked summarization for long transcripts.

pub mod anthropic;
pub mod ollama;
pub mod openai;
pub mod prompts;
pub mod provider;
pub mod summary;
