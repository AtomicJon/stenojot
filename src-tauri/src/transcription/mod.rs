//! Speech-to-text transcription with pluggable engine backends.
//!
//! This module manages model lifecycle (download, storage) and provides
//! a background worker that consumes audio ring buffers, runs inference
//! via the selected [`engine::SttBackend`], and streams transcript
//! segments to the frontend.
//!
//! Supported engines:
//! - **Whisper** — whisper.cpp GGML models via `whisper-rs`
//! - **Parakeet** — NVIDIA Parakeet TDT via ONNX Runtime
//! - **Moonshine** — Moonshine v2 via ONNX Runtime
//! - **SenseVoice** — Alibaba SenseVoice via ONNX Runtime

pub mod engine;
pub mod manager;
pub mod onnx_backend;
pub mod whisper_backend;
pub mod worker;
