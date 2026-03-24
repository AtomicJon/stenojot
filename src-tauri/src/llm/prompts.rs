//! Prompt templates for LLM-powered meeting summarization and title generation.
//!
//! Contains system and user prompt constants used by the summary orchestration
//! layer. Prompts are designed to produce structured Markdown output that
//! matches the StenoJot summary file format.

/// System prompt for single-pass or first-chunk meeting summarization.
pub const SUMMARY_SYSTEM: &str = "\
You are a meeting summarizer. Given a full meeting transcript, produce a concise \
summary in Markdown format with exactly two sections:\n\
\n\
## Key Points\n\
A bulleted list of substantive topics and decisions discussed. Filter out small talk, \
greetings, \"can you hear me\" moments, and filler. Focus on what matters.\n\
\n\
## Action Items\n\
A bulleted checklist using `- [ ]` format with assignee where identifiable \
(e.g. `- [ ] @Alex: File tickets by Friday`). If no action items are found, \
write `- No action items identified.`\n\
\n\
Be concise and factual. Do not include a title heading — just the two sections. \
Do not add commentary or preamble.";

/// System prompt for refining a running summary with the next transcript chunk.
pub const REFINEMENT_SYSTEM: &str = "\
You are a meeting summarizer performing iterative refinement. You will receive \
your running summary of the meeting so far, followed by the next portion of the \
transcript.\n\
\n\
Update and expand the summary to incorporate the new content. Maintain the exact \
same structure with two sections:\n\
\n\
## Key Points\n\
Merge new substantive topics and decisions into the existing list. Remove \
duplicates. Keep filtering out small talk and filler.\n\
\n\
## Action Items\n\
Add any new action items found. Keep existing ones. Use `- [ ]` format with \
assignees where identifiable.\n\
\n\
Output only the updated summary — no commentary, no preamble, just the two \
Markdown sections.";

/// Format the user prompt for the refinement pass.
///
/// Combines the running summary with the next transcript chunk into a single
/// user message for the LLM.
pub fn refinement_user_prompt(running_summary: &str, next_chunk: &str) -> String {
    format!(
        "Here is the running summary so far:\n\n\
         ---\n\
         {}\n\
         ---\n\n\
         Here is the next portion of the transcript:\n\n\
         {}",
        running_summary, next_chunk
    )
}

/// System prompt for generating a short meeting title from transcript content.
pub const TITLE_SYSTEM: &str = "\
Given a meeting transcript excerpt, generate a short descriptive title for this \
meeting (3-6 words). Return ONLY the title text, nothing else. No quotes, no \
punctuation except what is natural in the title.\n\
\n\
Examples of good titles: Sprint Planning, 1-on-1 with Alex, Q3 Budget Review, \
API Migration Discussion, Weekly Team Standup.\n\
\n\
If the transcript is too short or uninformative to determine a topic, respond \
with just: Meeting";

/// Maximum number of characters from the transcript to send for title generation.
///
/// Title generation only needs the beginning of the transcript to understand
/// the meeting topic, so we limit input to save tokens.
pub const TITLE_MAX_CHARS: usize = 2000;

/// Default chunk size in characters for splitting long transcripts.
///
/// Chosen to fit comfortably within most LLM context windows while leaving
/// room for the system prompt and response.
pub const DEFAULT_CHUNK_SIZE: usize = 30_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_system_prompt_is_non_empty() {
        // Arrange / Act / Assert
        assert!(!SUMMARY_SYSTEM.is_empty());
        assert!(SUMMARY_SYSTEM.contains("Key Points"));
        assert!(SUMMARY_SYSTEM.contains("Action Items"));
    }

    #[test]
    fn refinement_system_prompt_is_non_empty() {
        // Arrange / Act / Assert
        assert!(!REFINEMENT_SYSTEM.is_empty());
        assert!(REFINEMENT_SYSTEM.contains("running summary"));
        assert!(REFINEMENT_SYSTEM.contains("Key Points"));
    }

    #[test]
    fn title_system_prompt_is_non_empty() {
        // Arrange / Act / Assert
        assert!(!TITLE_SYSTEM.is_empty());
        assert!(TITLE_SYSTEM.contains("title"));
    }

    #[test]
    fn refinement_user_prompt_combines_summary_and_chunk() {
        // Arrange
        let summary = "## Key Points\n- Topic A";
        let chunk = "[00:10:00] **Me:** Let's discuss topic B.";

        // Act
        let result = refinement_user_prompt(summary, chunk);

        // Assert
        assert!(result.contains(summary));
        assert!(result.contains(chunk));
        assert!(result.contains("running summary so far"));
        assert!(result.contains("next portion"));
    }

    #[test]
    fn default_chunk_size_is_reasonable() {
        // Arrange / Act / Assert
        assert!(DEFAULT_CHUNK_SIZE >= 10_000);
        assert!(DEFAULT_CHUNK_SIZE <= 100_000);
    }

    #[test]
    fn title_max_chars_is_reasonable() {
        // Arrange / Act / Assert
        assert!(TITLE_MAX_CHARS >= 500);
        assert!(TITLE_MAX_CHARS <= 10_000);
    }
}
