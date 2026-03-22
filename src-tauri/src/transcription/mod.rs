//! Speech-to-text transcription using Whisper.
//!
//! This module manages model lifecycle (download, storage) and provides
//! a background worker that consumes audio ring buffers, runs Whisper
//! inference, and streams transcript segments to the frontend.

pub mod manager;
pub mod worker;
