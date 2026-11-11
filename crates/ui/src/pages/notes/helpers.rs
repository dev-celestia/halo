//! Helpers for notes text statistics, snippets, and relative timestamps.

use chrono::{DateTime, Utc};

/// Format an RFC3339 timestamp as a friendly relative time string (e.g. "just now", "5m ago").
pub fn format_relative_time(timestamp: &str) -> String {
    let Ok(parsed) = DateTime::parse_from_rfc3339(timestamp) else {
        return "recently".to_string();
    };
    let now = Utc::now();
    let diff = now.signed_duration_since(parsed.with_timezone(&Utc));

    if diff.num_seconds() < 60 {
        "just now".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours())
    } else if diff.num_days() == 1 {
        "yesterday".to_string()
    } else if diff.num_days() < 30 {
        format!("{}d ago", diff.num_days())
    } else {
        parsed.format("%b %d").to_string()
    }
}

/// Calculate word count and character count for a given text.
pub fn count_words_and_chars(text: &str) -> (usize, usize) {
    let char_count = text.chars().count();
    let word_count = text.split_whitespace().count();
    (word_count, char_count)
}

/// Extract a clean text snippet from markdown (strips headings, code blocks, bullet points).
pub fn extract_snippet(text: &str, max_len: usize) -> String {
    let mut clean_lines = Vec::new();
    let mut in_code_block = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block || trimmed.is_empty() {
            continue;
        }

        // Strip heading markers
        let stripped = trimmed
            .trim_start_matches('#')
            .trim_start_matches('*')
            .trim_start_matches('-')
            .trim_start_matches('>')
            .trim();

        if !stripped.is_empty() {
            clean_lines.push(stripped);
        }
    }

    let joined = clean_lines.join(" ");
    if joined.is_empty() {
        return "Empty note…".to_string();
    }

    if joined.chars().count() > max_len {
        let truncated: String = joined.chars().take(max_len).collect();
        format!("{truncated}…")
    } else {
        joined
    }
}
