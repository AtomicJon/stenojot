//! Summary generation orchestration.
//!
//! Coordinates reading the transcript, calling the LLM for title and summary
//! generation, writing the summary file, and renaming meeting files when the
//! LLM provides a better title than the fallback.

use chrono::{DateTime, Local};
use std::fs;
use std::path::{Path, PathBuf};

use super::prompts;
use super::provider::{create_client, LlmConfig, LlmError};
use crate::markdown;

/// Result of a successful summary generation.
#[derive(Debug, Clone)]
pub struct SummaryResult {
    /// Path to the written summary file.
    pub summary_path: PathBuf,
    /// LLM-generated meeting name (or original if generation failed/unchanged).
    pub meeting_name: String,
    /// Path to the transcript file (may have been renamed).
    pub transcript_path: PathBuf,
}

/// Generate a meeting summary from a transcript file.
///
/// Reads the transcript, calls the LLM for title and summary generation,
/// writes the summary Markdown file, and optionally renames meeting files
/// to use the LLM-generated title.
pub fn generate_summary(
    config: &LlmConfig,
    transcript_path: &Path,
    output_dir: &Path,
    start_time: DateTime<Local>,
    end_time: DateTime<Local>,
    current_meeting_name: &str,
) -> Result<SummaryResult, LlmError> {
    let transcript_content =
        fs::read_to_string(transcript_path).map_err(|e| LlmError::Network(e.to_string()))?;

    let client = create_client(config)?;

    // Strip the header (everything before the first "---" separator) to get just
    // the transcript body for the LLM
    let transcript_body = extract_transcript_body(&transcript_content);

    // Generate meeting title from the beginning of the transcript
    let title_input: String = transcript_body
        .chars()
        .take(prompts::TITLE_MAX_CHARS)
        .collect();
    let title = match client.complete(prompts::TITLE_SYSTEM, &title_input) {
        Ok(resp) => {
            let raw = resp.text.trim().to_string();
            let sanitized = markdown::sanitize_filename(&raw);
            if sanitized.is_empty() {
                current_meeting_name.to_string()
            } else {
                sanitized
            }
        }
        Err(_) => current_meeting_name.to_string(),
    };

    // Generate summary using chunked summarization
    let summary_body = summarize_chunked(client.as_ref(), transcript_body)?;

    // Format the full summary file content
    let summary_content = format_summary(&title, start_time, end_time, &summary_body);

    // Determine file paths and handle renames if the title changed
    let (final_transcript_path, summary_path) = if title != current_meeting_name {
        let rename_result = markdown::rename_meeting_files(
            transcript_path,
            output_dir,
            &start_time,
            current_meeting_name,
            &title,
        );
        match rename_result {
            Ok(new_transcript_path) => {
                let sp = markdown::summary_path_for_transcript(&new_transcript_path);
                (new_transcript_path, sp)
            }
            Err(_) => {
                // Rename failed, use original paths
                let sp = markdown::summary_path_for_transcript(transcript_path);
                (transcript_path.to_path_buf(), sp)
            }
        }
    } else {
        let sp = markdown::summary_path_for_transcript(transcript_path);
        (transcript_path.to_path_buf(), sp)
    };

    // Write summary file atomically
    markdown::write_summary(&summary_path, &summary_content)?;

    Ok(SummaryResult {
        summary_path,
        meeting_name: title,
        transcript_path: final_transcript_path,
    })
}

/// Extract the transcript body (everything after the `---` header separator).
///
/// If no separator is found, returns the full content.
fn extract_transcript_body(content: &str) -> &str {
    if let Some(pos) = content.find("\n---\n") {
        let body_start = pos + 5; // skip "\n---\n"
        &content[body_start..]
    } else {
        content
    }
}

/// Summarize transcript content using chunked iterative refinement.
///
/// For short transcripts, sends the full text in a single LLM call.
/// For longer transcripts, splits into chunks at speaker boundaries
/// and iteratively refines the running summary with each new chunk.
fn summarize_chunked(
    client: &dyn super::provider::LlmClient,
    transcript_body: &str,
) -> Result<String, LlmError> {
    let chunks = chunk_transcript(transcript_body, prompts::DEFAULT_CHUNK_SIZE);

    if chunks.is_empty() {
        return Err(LlmError::ParseError("Empty transcript".to_string()));
    }

    // First chunk: standard summary prompt
    let mut running_summary = client.complete(prompts::SUMMARY_SYSTEM, chunks[0])?.text;

    // Remaining chunks: refinement prompt
    for chunk in &chunks[1..] {
        let user_prompt = prompts::refinement_user_prompt(&running_summary, chunk);
        running_summary = client
            .complete(prompts::REFINEMENT_SYSTEM, &user_prompt)?
            .text;
    }

    Ok(running_summary)
}

/// Split transcript text into chunks at speaker turn boundaries.
///
/// Chunks are at most `max_chars` long. Splits happen at `\n[` boundaries
/// (the start of timestamped speaker lines) to avoid cutting mid-sentence.
/// If a single speaker turn exceeds `max_chars`, it is included as-is in
/// its own chunk.
pub fn chunk_transcript(text: &str, max_chars: usize) -> Vec<&str> {
    if text.len() <= max_chars {
        return vec![text];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < text.len() {
        if start + max_chars >= text.len() {
            chunks.push(&text[start..]);
            break;
        }

        // Look for the last speaker turn boundary within the limit
        let search_end = start + max_chars;
        let search_region = &text[start..search_end];

        // Find the last "\n[" in the allowed region (start of a timestamp line)
        let split_pos = if let Some(last_boundary) = search_region.rfind("\n[") {
            start + last_boundary + 1 // split at the "[" character
        } else {
            // No speaker boundary found; split at max_chars
            search_end
        };

        chunks.push(&text[start..split_pos]);
        start = split_pos;
    }

    chunks
}

/// Format the summary Markdown content.
///
/// Includes a relative link to the corresponding transcript file in the
/// `transcripts/` sibling directory.
pub fn format_summary(
    title: &str,
    start_time: DateTime<Local>,
    end_time: DateTime<Local>,
    summary_body: &str,
) -> String {
    let tx_filename = markdown::transcript_filename(&start_time, title);
    let encoded_filename = tx_filename.replace(' ', "%20");

    format!(
        "# {}\n\n**Date:** {}–{}\n**Transcript:** [View Transcript](../transcripts/{})\n\n{}\n",
        title,
        start_time.format("%Y-%m-%d %H:%M"),
        end_time.format("%H:%M"),
        encoded_filename,
        summary_body.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_transcript_body_with_header() {
        // Arrange
        let content = "# Meeting — Full Transcript\n\n**Date:** 2026-03-22\n\n---\n\n[00:00:01] **Me:** Hello";

        // Act
        let body = extract_transcript_body(content);

        // Assert
        assert_eq!(body, "\n[00:00:01] **Me:** Hello");
    }

    #[test]
    fn extract_transcript_body_without_header() {
        // Arrange
        let content = "[00:00:01] **Me:** Hello";

        // Act
        let body = extract_transcript_body(content);

        // Assert
        assert_eq!(body, content);
    }

    #[test]
    fn chunk_transcript_short_returns_single() {
        // Arrange
        let text = "[00:00:01] **Me:** Short meeting.";

        // Act
        let chunks = chunk_transcript(text, 1000);

        // Assert
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], text);
    }

    #[test]
    fn chunk_transcript_splits_at_speaker_boundary() {
        // Arrange
        let text = "[00:00:01] **Me:** First line.\n\n[00:00:10] **Others:** Second line.\n\n[00:00:20] **Me:** Third line.";

        // Act — set max_chars so it splits after the second speaker turn
        let chunks = chunk_transcript(text, 60);

        // Assert
        assert!(chunks.len() >= 2);
        // Each chunk should start with "[" (a timestamp) except possibly the first
        for chunk in &chunks[1..] {
            assert!(
                chunk.starts_with('['),
                "Chunk should start with timestamp: {}",
                chunk
            );
        }
    }

    #[test]
    fn chunk_transcript_handles_no_boundaries() {
        // Arrange — one very long line with no speaker boundaries
        let text = "a".repeat(100);

        // Act
        let chunks = chunk_transcript(&text, 30);

        // Assert
        assert!(chunks.len() > 1);
        // Total length should match original
        let total: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn format_summary_produces_valid_markdown() {
        // Arrange
        let start = chrono::Local::now();
        let end = start;
        let body = "## Key Points\n- Item 1\n\n## Action Items\n- [ ] @Me: Do thing";

        // Act
        let result = format_summary("Sprint Planning", start, end, body);

        // Assert
        assert!(result.starts_with("# Sprint Planning\n"));
        assert!(result.contains("**Date:**"));
        assert!(result.contains("**Transcript:** [View Transcript](../transcripts/"));
        assert!(result.contains("Sprint%20Planning"));
        assert!(result.contains("## Key Points"));
        assert!(result.contains("## Action Items"));
    }

    #[test]
    fn format_summary_trims_body_whitespace() {
        // Arrange
        let start = chrono::Local::now();
        let end = start;
        let body = "\n\n## Key Points\n- Item\n\n";

        // Act
        let result = format_summary("Test", start, end, body);

        // Assert
        // Should not have double blank lines before Key Points
        assert!(!result.contains("\n\n\n"));
    }

    #[test]
    fn format_summary_includes_transcript_link() {
        // Arrange
        let start = chrono::Local::now();
        let end = start;

        // Act
        let result = format_summary("My Meeting", start, end, "Summary body");

        // Assert
        assert!(result.contains("../transcripts/"));
        assert!(result.contains("-%20Transcript.md"));
    }
}
