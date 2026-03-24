//! Markdown transcript file generation and meeting listing.
//!
//! Formats transcript segments into the StenoJot Markdown format and writes
//! them to the configured output directory. Also provides meeting listing by
//! parsing transcript filenames from the output directory.

use chrono::{DateTime, Local};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

use crate::audio::types::TranscriptSegment;
use crate::llm::provider::LlmError;

/// Subdirectory name for transcript files within the output directory.
pub const TRANSCRIPTS_SUBDIR: &str = "transcripts";

/// Subdirectory name for summary files within the output directory.
pub const SUMMARIES_SUBDIR: &str = "summaries";

/// Filename suffix for transcript files.
pub const TRANSCRIPT_SUFFIX: &str = " - Transcript.md";

/// A single meeting entry parsed from the output directory.
#[derive(Debug, Clone, Serialize)]
pub struct MeetingEntry {
    /// Display name of the meeting.
    pub name: String,
    /// Date string (YYYY-MM-DD).
    pub date: String,
    /// Time string (HH:MM).
    pub time: String,
    /// Whether the transcript file exists.
    pub has_transcript: bool,
    /// Whether the summary file exists (Phase 4).
    pub has_summary: bool,
    /// Absolute path to the transcript file.
    pub transcript_path: String,
    /// Absolute path to the summary file (empty if not present).
    pub summary_path: String,
    /// Transcript file size in bytes.
    pub size_bytes: u64,
}

/// Generate a fallback meeting name from the recording start time.
pub fn resolve_meeting_name(start_time: &DateTime<Local>) -> String {
    format!("Meeting at {}", start_time.format("%H-%M"))
}

/// Strip filesystem-unsafe characters from a filename component.
pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'))
        .collect::<String>()
        .trim()
        .to_string()
}

/// Percent-encode spaces in a filename for use in Markdown links.
fn encode_filename_for_link(filename: &str) -> String {
    filename.replace(' ', "%20")
}

/// Generate the summary filename from the meeting start time and name.
///
/// Format: `YYYY-MM-DD HH.MM <Meeting Name>.md`
pub fn summary_filename(start_time: &DateTime<Local>, meeting_name: &str) -> String {
    let safe_name = sanitize_filename(meeting_name);
    format!("{} {}.md", start_time.format("%Y-%m-%d %H.%M"), safe_name)
}

/// Format milliseconds as `[HH:MM:SS]` for transcript entries.
pub fn format_timestamp_hhmmss(ms: u64) -> String {
    let total_seconds = ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("[{:02}:{:02}:{:02}]", hours, minutes, seconds)
}

/// Format transcript segments into a Markdown string.
///
/// Includes a relative link to the corresponding summary file in the
/// `summaries/` sibling directory.
pub fn format_transcript(
    segments: &[TranscriptSegment],
    meeting_name: &str,
    start_time: DateTime<Local>,
    end_time: DateTime<Local>,
) -> String {
    let mut md = String::new();

    let sum_filename = summary_filename(&start_time, meeting_name);
    let sum_link = encode_filename_for_link(&sum_filename);

    md.push_str(&format!("# {} — Full Transcript\n\n", meeting_name));
    md.push_str(&format!(
        "**Date:** {}–{}\n",
        start_time.format("%Y-%m-%d %H:%M"),
        end_time.format("%H:%M")
    ));
    md.push_str("**Participants:** Me, Others\n");
    md.push_str(&format!(
        "**Summary:** [View Summary](../summaries/{})\n\n",
        sum_link
    ));
    md.push_str("---\n\n");

    for seg in segments {
        let timestamp = format_timestamp_hhmmss(seg.start_ms);
        let speaker = match seg.speaker {
            crate::audio::types::Speaker::Me => "Me",
            crate::audio::types::Speaker::Others => "Others",
        };
        md.push_str(&format!("{} **{}:** {}\n\n", timestamp, speaker, seg.text));
    }

    md
}

/// Generate the transcript filename from the meeting start time and name.
///
/// Format: `YYYY-MM-DD HH.MM <Meeting Name> - Transcript.md`
pub fn transcript_filename(start_time: &DateTime<Local>, meeting_name: &str) -> String {
    let safe_name = sanitize_filename(meeting_name);
    format!(
        "{} {}{}",
        start_time.format("%Y-%m-%d %H.%M"),
        safe_name,
        TRANSCRIPT_SUFFIX
    )
}

/// Write a transcript Markdown file to the `transcripts/` subdirectory.
///
/// Creates the subdirectory if it does not exist. Uses a temporary file
/// and rename for atomicity.
pub fn write_transcript(
    output_dir: &Path,
    segments: &[TranscriptSegment],
    meeting_name: &str,
    start_time: DateTime<Local>,
    end_time: DateTime<Local>,
) -> Result<PathBuf, String> {
    let transcripts_dir = output_dir.join(TRANSCRIPTS_SUBDIR);
    fs::create_dir_all(&transcripts_dir)
        .map_err(|e| format!("Failed to create transcripts directory: {}", e))?;

    let filename = transcript_filename(&start_time, meeting_name);
    let path = transcripts_dir.join(&filename);
    let tmp_path = transcripts_dir.join(format!("{}.tmp", filename));

    let content = format_transcript(segments, meeting_name, start_time, end_time);

    fs::write(&tmp_path, &content)
        .map_err(|e| format!("Failed to write transcript file: {}", e))?;

    fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to rename transcript file: {}", e))?;

    eprintln!("Transcript saved to {}", path.display());
    Ok(path)
}

/// Rewrite an existing transcript file with updated segments.
///
/// Uses a temporary file and rename for atomicity.
pub fn update_transcript(
    path: &Path,
    segments: &[TranscriptSegment],
    meeting_name: &str,
    start_time: DateTime<Local>,
    end_time: DateTime<Local>,
) -> Result<(), String> {
    let content = format_transcript(segments, meeting_name, start_time, end_time);
    let tmp_path = path.with_extension("md.tmp");

    fs::write(&tmp_path, &content)
        .map_err(|e| format!("Failed to write transcript file: {}", e))?;

    fs::rename(&tmp_path, path)
        .map_err(|e| format!("Failed to rename transcript file: {}", e))?;

    Ok(())
}

/// Compute the summary file path for a given transcript file path.
///
/// Navigates from the `transcripts/` subdirectory to the sibling `summaries/`
/// subdirectory. Transforms `transcripts/YYYY-MM-DD HH.MM Name - Transcript.md`
/// → `summaries/YYYY-MM-DD HH.MM Name.md`.
pub fn summary_path_for_transcript(transcript_path: &Path) -> PathBuf {
    let filename = transcript_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    let sum_filename = if let Some(stem) = filename.strip_suffix(TRANSCRIPT_SUFFIX) {
        format!("{}.md", stem)
    } else {
        // Fallback: just replace extension
        format!("{}.summary.md", filename.trim_end_matches(".md"))
    };

    // Navigate from transcripts/ up to the output root, then into summaries/
    transcript_path
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(Path::new("."))
        .join(SUMMARIES_SUBDIR)
        .join(sum_filename)
}

/// Write a summary Markdown file atomically (temp file + rename).
///
/// Creates the parent directory (typically `summaries/`) if it does not exist.
pub fn write_summary(path: &Path, content: &str) -> Result<(), LlmError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            LlmError::Network(format!("Failed to create summaries directory: {}", e))
        })?;
    }

    let tmp_path = path.with_extension("md.tmp");

    fs::write(&tmp_path, content)
        .map_err(|e| LlmError::Network(format!("Failed to write summary file: {}", e)))?;

    fs::rename(&tmp_path, path)
        .map_err(|e| LlmError::Network(format!("Failed to rename summary file: {}", e)))?;

    eprintln!("Summary saved to {}", path.display());
    Ok(())
}

/// Rename meeting files when the LLM generates a better title.
///
/// Renames the transcript file in `transcripts/` (and summary file in
/// `summaries/` if it exists) from the old meeting name to the new one.
/// Also updates the summary cross-link inside the transcript content.
/// Returns the new transcript path.
pub fn rename_meeting_files(
    transcript_path: &Path,
    output_dir: &Path,
    start_time: &DateTime<Local>,
    old_name: &str,
    new_name: &str,
) -> Result<PathBuf, String> {
    let new_transcript_fname = transcript_filename(start_time, new_name);
    let transcripts_dir = output_dir.join(TRANSCRIPTS_SUBDIR);
    let new_transcript_path = transcripts_dir.join(&new_transcript_fname);

    // Don't rename if the path hasn't changed
    if transcript_path == new_transcript_path {
        return Ok(transcript_path.to_path_buf());
    }

    // Don't overwrite an existing file
    if new_transcript_path.exists() {
        return Err(format!(
            "Cannot rename: {} already exists",
            new_transcript_path.display()
        ));
    }

    fs::rename(transcript_path, &new_transcript_path)
        .map_err(|e| format!("Failed to rename transcript: {}", e))?;

    // Also rename summary file if it exists
    let old_summary = summary_path_for_transcript(transcript_path);
    if old_summary.exists() {
        let new_summary = summary_path_for_transcript(&new_transcript_path);
        if !new_summary.exists() {
            fs::rename(&old_summary, &new_summary)
                .map_err(|e| format!("Failed to rename summary: {}", e))?;
        }
    }

    // Update the summary cross-link inside the transcript content
    let old_sum_filename = summary_filename(start_time, old_name);
    let new_sum_filename = summary_filename(start_time, new_name);
    let old_link = encode_filename_for_link(&old_sum_filename);
    let new_link = encode_filename_for_link(&new_sum_filename);
    if old_link != new_link {
        if let Ok(content) = fs::read_to_string(&new_transcript_path) {
            let updated = content.replace(&old_link, &new_link);
            if updated != content {
                let _ = fs::write(&new_transcript_path, &updated);
            }
        }
    }

    eprintln!(
        "Renamed transcript: {} → {}",
        transcript_path.display(),
        new_transcript_path.display()
    );

    Ok(new_transcript_path)
}

/// List meetings by scanning the `transcripts/` subdirectory.
///
/// Returns entries sorted newest-first. Only files matching the
/// `YYYY-MM-DD HH.MM <Name> - Transcript.md` pattern are included.
/// Summary existence is checked in the sibling `summaries/` subdirectory.
pub fn list_meetings_in_dir(output_dir: &Path) -> Vec<MeetingEntry> {
    let transcripts_dir = output_dir.join(TRANSCRIPTS_SUBDIR);
    let entries = match fs::read_dir(&transcripts_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let summaries_dir = output_dir.join(SUMMARIES_SUBDIR);

    let mut meetings: Vec<MeetingEntry> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let filename = entry.file_name().to_string_lossy().to_string();
            parse_transcript_filename(
                &filename,
                entry.path().to_string_lossy().as_ref(),
                &summaries_dir,
            )
        })
        .collect();

    // Sort newest first (by date + time descending)
    meetings.sort_by(|a, b| {
        let a_key = format!("{} {}", a.date, a.time);
        let b_key = format!("{} {}", b.date, b.time);
        b_key.cmp(&a_key)
    });

    meetings
}

/// Parse a transcript filename into a `MeetingEntry`.
///
/// Expected format: `YYYY-MM-DD HH.MM <Name> - Transcript.md`.
/// Summary existence is checked in `summaries_dir`.
fn parse_transcript_filename(
    filename: &str,
    full_path: &str,
    summaries_dir: &Path,
) -> Option<MeetingEntry> {
    let stem = filename.strip_suffix(TRANSCRIPT_SUFFIX)?;

    // Need at least "YYYY-MM-DD HH.MM X" = 17+ chars
    if stem.len() < 17 {
        return None;
    }

    let date = &stem[..10]; // YYYY-MM-DD
    let time_raw = stem.get(11..16)?; // HH.MM

    // Validate date format
    if date.len() != 10 || &date[4..5] != "-" || &date[7..8] != "-" {
        return None;
    }
    // Validate time format
    if time_raw.len() != 5 || &time_raw[2..3] != "." {
        return None;
    }

    let time = time_raw.replace('.', ":");
    let name = stem.get(17..)?.trim().to_string();

    if name.is_empty() {
        return None;
    }

    // Check for corresponding summary file in summaries/ directory
    let sum_filename = format!("{}.md", stem);
    let summary_path = summaries_dir.join(&sum_filename);
    let has_summary = summary_path.exists();

    let size_bytes = fs::metadata(full_path).map(|m| m.len()).unwrap_or(0);

    Some(MeetingEntry {
        name,
        date: date.to_string(),
        time,
        has_transcript: true,
        has_summary,
        transcript_path: full_path.to_string(),
        summary_path: if has_summary {
            summary_path.to_string_lossy().to_string()
        } else {
            String::new()
        },
        size_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::types::Speaker;

    #[test]
    fn resolve_meeting_name_uses_time() {
        // Arrange
        let time = chrono::Local::now();

        // Act
        let name = resolve_meeting_name(&time);

        // Assert
        assert!(name.starts_with("Meeting at "));
        assert_eq!(name.len(), "Meeting at HH-MM".len());
    }

    #[test]
    fn sanitize_filename_strips_unsafe_chars() {
        // Arrange
        let name = "Meeting: Q&A <2026> | \"Team\"";

        // Act
        let result = sanitize_filename(name);

        // Assert
        assert_eq!(result, "Meeting Q&A 2026  Team");
    }

    #[test]
    fn sanitize_filename_preserves_normal_chars() {
        // Arrange
        let name = "Sprint Planning 2026-03-22";

        // Act
        let result = sanitize_filename(name);

        // Assert
        assert_eq!(result, name);
    }

    #[test]
    fn format_timestamp_hhmmss_zero() {
        // Arrange & Act
        let result = format_timestamp_hhmmss(0);

        // Assert
        assert_eq!(result, "[00:00:00]");
    }

    #[test]
    fn format_timestamp_hhmmss_complex() {
        // Arrange — 1h 1m 1s = 3661000ms
        let ms = 3_661_000;

        // Act
        let result = format_timestamp_hhmmss(ms);

        // Assert
        assert_eq!(result, "[01:01:01]");
    }

    #[test]
    fn format_timestamp_hhmmss_minutes_only() {
        // Arrange — 5m 30s = 330000ms
        let ms = 330_000;

        // Act
        let result = format_timestamp_hhmmss(ms);

        // Assert
        assert_eq!(result, "[00:05:30]");
    }

    #[test]
    fn format_transcript_empty_segments() {
        // Arrange
        let start = chrono::Local::now();
        let end = start;

        // Act
        let result = format_transcript(&[], "Test Meeting", start, end);

        // Assert
        assert!(result.contains("# Test Meeting — Full Transcript"));
        assert!(result.contains("**Participants:** Me, Others"));
        assert!(result.contains("**Summary:** [View Summary](../summaries/"));
        assert!(result.contains("---"));
    }

    #[test]
    fn format_transcript_with_segments() {
        // Arrange
        let segments = vec![
            TranscriptSegment {
                text: "Hello everyone.".to_string(),
                speaker: Speaker::Me,
                start_ms: 12_000,
                end_ms: 15_000,
                is_final: true,
            },
            TranscriptSegment {
                text: "Hi there.".to_string(),
                speaker: Speaker::Others,
                start_ms: 16_000,
                end_ms: 18_000,
                is_final: true,
            },
        ];
        let start = chrono::Local::now();
        let end = start;

        // Act
        let result = format_transcript(&segments, "Standup", start, end);

        // Assert
        assert!(result.contains("[00:00:12] **Me:** Hello everyone."));
        assert!(result.contains("[00:00:16] **Others:** Hi there."));
        assert!(result.contains("**Summary:** [View Summary](../summaries/"));
    }

    #[test]
    fn write_transcript_creates_file() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let segments = vec![TranscriptSegment {
            text: "Test segment.".to_string(),
            speaker: Speaker::Me,
            start_ms: 0,
            end_ms: 5000,
            is_final: true,
        }];
        let start = chrono::Local::now();
        let end = start;

        // Act
        let path = write_transcript(tmp.path(), &segments, "Test", start, end).unwrap();

        // Assert
        assert!(path.exists());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("Test segment."));
        assert!(path.to_string_lossy().ends_with("- Transcript.md"));
        // File should be inside the transcripts/ subdirectory
        assert!(path.parent().unwrap().ends_with(TRANSCRIPTS_SUBDIR));
    }

    #[test]
    fn write_transcript_creates_output_directory() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("deep").join("output");
        let segments = vec![TranscriptSegment {
            text: "Hello.".to_string(),
            speaker: Speaker::Me,
            start_ms: 0,
            end_ms: 1000,
            is_final: true,
        }];
        let start = chrono::Local::now();
        let end = start;

        // Act
        let result = write_transcript(&nested, &segments, "Test", start, end);

        // Assert
        assert!(result.is_ok());
        assert!(nested.join(TRANSCRIPTS_SUBDIR).exists());
    }

    #[test]
    fn parse_transcript_filename_valid() {
        // Arrange
        let filename = "2026-03-22 14.00 Meeting at 14-00 - Transcript.md";
        let summaries_dir = Path::new("/output/summaries");

        // Act
        let entry = parse_transcript_filename(
            filename,
            "/output/transcripts/2026-03-22 14.00 Meeting at 14-00 - Transcript.md",
            summaries_dir,
        );

        // Assert
        let entry = entry.unwrap();
        assert_eq!(entry.date, "2026-03-22");
        assert_eq!(entry.time, "14:00");
        assert_eq!(entry.name, "Meeting at 14-00");
    }

    #[test]
    fn parse_transcript_filename_rejects_non_transcript() {
        // Arrange
        let filename = "2026-03-22 14.00 Meeting at 14-00.md";
        let summaries_dir = Path::new("/output/summaries");

        // Act
        let entry = parse_transcript_filename(filename, "/output/some.md", summaries_dir);

        // Assert
        assert!(entry.is_none());
    }

    #[test]
    fn parse_transcript_filename_rejects_malformed() {
        // Arrange
        let filename = "random notes.md";
        let summaries_dir = Path::new("/output/summaries");

        // Act
        let entry = parse_transcript_filename(filename, "/output/random.md", summaries_dir);

        // Assert
        assert!(entry.is_none());
    }

    #[test]
    fn list_meetings_in_dir_empty() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();

        // Act
        let meetings = list_meetings_in_dir(tmp.path());

        // Assert
        assert!(meetings.is_empty());
    }

    #[test]
    fn list_meetings_in_dir_finds_transcripts() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let transcripts_dir = tmp.path().join(TRANSCRIPTS_SUBDIR);
        fs::create_dir_all(&transcripts_dir).unwrap();
        fs::write(
            transcripts_dir.join("2026-03-22 14.00 Standup - Transcript.md"),
            "# content",
        )
        .unwrap();
        fs::write(
            transcripts_dir.join("2026-03-22 15.30 Planning - Transcript.md"),
            "# content",
        )
        .unwrap();
        // Non-matching file should be ignored
        fs::write(transcripts_dir.join("notes.txt"), "hello").unwrap();

        // Act
        let meetings = list_meetings_in_dir(tmp.path());

        // Assert
        assert_eq!(meetings.len(), 2);
        // Newest first
        assert_eq!(meetings[0].name, "Planning");
        assert_eq!(meetings[1].name, "Standup");
    }

    #[test]
    fn update_transcript_rewrites_file() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let start = chrono::Local::now();
        let path = write_transcript(tmp.path(), &[], "Test", start, start).unwrap();
        let initial = fs::read_to_string(&path).unwrap();
        assert!(!initial.contains("Updated text."));

        let segments = vec![TranscriptSegment {
            text: "Updated text.".to_string(),
            speaker: Speaker::Me,
            start_ms: 0,
            end_ms: 5000,
            is_final: true,
        }];

        // Act
        update_transcript(&path, &segments, "Test", start, start).unwrap();

        // Assert
        let updated = fs::read_to_string(&path).unwrap();
        assert!(updated.contains("Updated text."));
    }

    #[test]
    fn update_transcript_preserves_filename() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let start = chrono::Local::now();
        let path = write_transcript(tmp.path(), &[], "Demo", start, start).unwrap();
        let filename_before = path.file_name().unwrap().to_owned();

        // Act
        update_transcript(&path, &[], "Demo", start, start).unwrap();

        // Assert — file is still at the same path
        assert!(path.exists());
        assert_eq!(path.file_name().unwrap(), filename_before);
    }

    #[test]
    fn list_meetings_in_dir_nonexistent_returns_empty() {
        // Arrange
        let path = PathBuf::from("/nonexistent/directory/12345");

        // Act
        let meetings = list_meetings_in_dir(&path);

        // Assert
        assert!(meetings.is_empty());
    }

    #[test]
    fn summary_path_for_transcript_standard() {
        // Arrange — transcript lives in transcripts/ subdir
        let transcript = PathBuf::from(
            "/output/transcripts/2026-03-22 14.00 Sprint Planning - Transcript.md",
        );

        // Act
        let summary = summary_path_for_transcript(&transcript);

        // Assert — summary goes to sibling summaries/ subdir
        assert_eq!(
            summary,
            PathBuf::from("/output/summaries/2026-03-22 14.00 Sprint Planning.md")
        );
    }

    #[test]
    fn summary_path_for_transcript_non_standard_filename() {
        // Arrange
        let transcript = PathBuf::from("/output/transcripts/random-notes.md");

        // Act
        let summary = summary_path_for_transcript(&transcript);

        // Assert
        assert_eq!(
            summary,
            PathBuf::from("/output/summaries/random-notes.summary.md")
        );
    }

    #[test]
    fn write_summary_creates_file_and_directory() {
        // Arrange — path inside a summaries/ subdir that doesn't exist yet
        let tmp = tempfile::tempdir().unwrap();
        let summaries_dir = tmp.path().join(SUMMARIES_SUBDIR);
        let path = summaries_dir.join("2026-03-22 14.00 Test.md");
        let content = "# Test\n\n## Key Points\n- Item 1\n";

        // Act
        let result = write_summary(&path, content);

        // Assert
        assert!(result.is_ok());
        assert!(path.exists());
        assert!(summaries_dir.exists());
        let written = fs::read_to_string(&path).unwrap();
        assert_eq!(written, content);
    }

    #[test]
    fn rename_meeting_files_renames_transcript() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let start = chrono::Local::now();
        let old_path = write_transcript(tmp.path(), &[], "Meeting at 14-00", start, start).unwrap();
        assert!(old_path.exists());

        // Act
        let new_path = rename_meeting_files(
            &old_path,
            tmp.path(),
            &start,
            "Meeting at 14-00",
            "Sprint Planning",
        )
        .unwrap();

        // Assert
        assert!(!old_path.exists());
        assert!(new_path.exists());
        assert!(new_path.to_string_lossy().contains("Sprint Planning"));
        // Verify summary link was updated in transcript content
        let content = fs::read_to_string(&new_path).unwrap();
        assert!(content.contains("Sprint%20Planning"));
        assert!(!content.contains("Meeting%20at%2014-00"));
    }

    #[test]
    fn rename_meeting_files_same_name_returns_original() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let start = chrono::Local::now();
        let path = write_transcript(tmp.path(), &[], "Standup", start, start).unwrap();

        // Act
        let result = rename_meeting_files(&path, tmp.path(), &start, "Standup", "Standup").unwrap();

        // Assert
        assert_eq!(result, path);
        assert!(path.exists());
    }

    #[test]
    fn rename_meeting_files_also_renames_summary() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let start = chrono::Local::now();
        let transcript_path =
            write_transcript(tmp.path(), &[], "Meeting at 14-00", start, start).unwrap();
        let old_summary_path = summary_path_for_transcript(&transcript_path);
        fs::create_dir_all(old_summary_path.parent().unwrap()).unwrap();
        fs::write(&old_summary_path, "# summary").unwrap();
        assert!(old_summary_path.exists());

        // Act
        let new_transcript = rename_meeting_files(
            &transcript_path,
            tmp.path(),
            &start,
            "Meeting at 14-00",
            "Sprint Planning",
        )
        .unwrap();

        // Assert
        let new_summary_path = summary_path_for_transcript(&new_transcript);
        assert!(new_summary_path.exists());
        assert!(!old_summary_path.exists());
    }

    #[test]
    fn list_meetings_detects_summary() {
        // Arrange — transcript in transcripts/, summary in summaries/
        let tmp = tempfile::tempdir().unwrap();
        let transcripts_dir = tmp.path().join(TRANSCRIPTS_SUBDIR);
        let summaries_dir = tmp.path().join(SUMMARIES_SUBDIR);
        fs::create_dir_all(&transcripts_dir).unwrap();
        fs::create_dir_all(&summaries_dir).unwrap();

        let transcript_name = "2026-03-22 14.00 Standup - Transcript.md";
        let summary_name = "2026-03-22 14.00 Standup.md";
        fs::write(transcripts_dir.join(transcript_name), "# content").unwrap();
        fs::write(summaries_dir.join(summary_name), "# summary").unwrap();

        // Act
        let meetings = list_meetings_in_dir(tmp.path());

        // Assert
        assert_eq!(meetings.len(), 1);
        assert!(meetings[0].has_summary);
        assert!(!meetings[0].summary_path.is_empty());
    }

    #[test]
    fn summary_filename_generates_correct_name() {
        // Arrange
        let start = chrono::Local::now();
        let expected_prefix = start.format("%Y-%m-%d %H.%M").to_string();

        // Act
        let result = summary_filename(&start, "Sprint Planning");

        // Assert
        assert_eq!(result, format!("{} Sprint Planning.md", expected_prefix));
    }

    #[test]
    fn encode_filename_for_link_encodes_spaces() {
        // Arrange
        let filename = "2026-03-22 14.00 Sprint Planning.md";

        // Act
        let result = encode_filename_for_link(filename);

        // Assert
        assert_eq!(result, "2026-03-22%2014.00%20Sprint%20Planning.md");
        assert!(!result.contains(' '));
    }
}
