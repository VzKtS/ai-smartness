//! Transcript reader — parse Claude Code JSONL transcripts for context token tracking.
//!
//! Reads the last usage block from `~/.claude/projects/{project_dir}/{session_id}.jsonl`.
//! Uses seek-from-end with adaptive read sizes (32K → 128K → 512K) for efficiency.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Default context window size (all current models).
const DEFAULT_WINDOW_SIZE: u64 = 200_000;

/// Parsed context token information from a transcript entry.
#[derive(Debug, Clone)]
pub struct ContextInfo {
    pub total_tokens: u64,
    pub percent: f64,
    pub cache_creation: u64,
    pub cache_read: u64,
    pub input: u64,
    pub output: u64,
    pub window_size: u64,
    pub model: Option<String>,
}

/// Find the transcript JSONL file for a given session_id.
/// Scans `~/.claude/projects/*/` for `{session_id}.jsonl`.
pub fn find_transcript(session_id: &str) -> Option<PathBuf> {
    let claude_projects = dirs::home_dir()?.join(".claude/projects");
    let filename = format!("{}.jsonl", session_id);

    let entries = std::fs::read_dir(&claude_projects).ok()?;
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let transcript = dir.join(&filename);
        if transcript.exists() {
            return Some(transcript);
        }
    }
    None
}

/// Read the last usage values from a transcript file.
/// Uses seek-from-end with adaptive chunk sizes to handle large files efficiently.
///
/// Parses 4 token fields from the last occurrence:
///   cache_creation_input_tokens + cache_read_input_tokens + input_tokens + output_tokens
///
/// Returns None if the file is unreadable or fields not found.
pub fn read_last_usage(transcript_path: &Path) -> Option<ContextInfo> {
    let file = std::fs::File::open(transcript_path).ok()?;
    let metadata = file.metadata().ok()?;
    let file_size = metadata.len();

    if file_size == 0 {
        return None;
    }

    // Adaptive read sizes: 32K → 128K → 512K
    let read_sizes: &[u64] = &[32_000, 128_000, 512_000];

    for &chunk_size in read_sizes {
        let read_size = file_size.min(chunk_size);
        let tail = read_tail(&file, file_size, read_size)?;

        if let Some(info) = parse_last_usage(&tail) {
            return Some(info);
        }

        // If we read the entire file and still didn't find it, give up
        if read_size >= file_size {
            break;
        }
    }

    None
}

/// Read the last `read_size` bytes from a file.
fn read_tail(file: &std::fs::File, file_size: u64, read_size: u64) -> Option<String> {
    let mut reader = std::io::BufReader::new(file);
    let offset = file_size.saturating_sub(read_size);
    reader.seek(SeekFrom::Start(offset)).ok()?;

    let mut tail = String::new();
    reader.read_to_string(&mut tail).ok()?;
    Some(tail)
}

/// Parse last occurrence of token fields using rfind (no regex crate needed).
fn parse_last_usage(content: &str) -> Option<ContextInfo> {
    let cache_creation = find_last_json_number(content, "\"cache_creation_input_tokens\":")?;
    let cache_read = find_last_json_number(content, "\"cache_read_input_tokens\":")?;
    let input = find_last_json_number(content, "\"input_tokens\":")?;
    let output = find_last_json_number(content, "\"output_tokens\":")?;

    // Dynamic window size detection: if tokens > 200K, user is on 1M beta
    let total = cache_creation + cache_read + input + output;
    let window_size = if total > DEFAULT_WINDOW_SIZE {
        1_000_000
    } else {
        DEFAULT_WINDOW_SIZE
    };

    let percent = (total as f64 / window_size as f64) * 100.0;

    let model = find_last_json_string(content, "model");

    Some(ContextInfo {
        total_tokens: total,
        percent,
        cache_creation,
        cache_read,
        input,
        output,
        window_size,
        model,
    })
}

/// Find the last occurrence of `"key":NUMBER` in content and parse the number.
fn find_last_json_number(content: &str, key: &str) -> Option<u64> {
    let idx = content.rfind(key)?;
    let after = &content[idx + key.len()..];
    // Skip whitespace after the colon
    let trimmed = after.trim_start();
    // Parse digits
    let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

/// Find the last occurrence of `"key":"VALUE"` in content and return the string value.
fn find_last_json_string(content: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\":\"", key);
    let pos = content.rfind(&pattern)?;
    let start = pos + pattern.len();
    let end = content[start..].find('"')? + start;
    Some(content[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_find_last_json_number() {
        let content = r#""input_tokens":100,"output_tokens":24,"input_tokens":500"#;
        // Should find the LAST occurrence of input_tokens (500)
        assert_eq!(find_last_json_number(content, "\"input_tokens\":"), Some(500));
        assert_eq!(find_last_json_number(content, "\"output_tokens\":"), Some(24));
        assert_eq!(find_last_json_number(content, "\"missing\":"), None);
    }

    #[test]
    fn test_find_last_json_number_with_spaces() {
        let content = r#""input_tokens": 42"#;
        assert_eq!(find_last_json_number(content, "\"input_tokens\":"), Some(42));
    }

    #[test]
    fn test_find_last_json_string() {
        let content = r#""model":"claude-3-opus","model":"claude-sonnet-4-20250514""#;
        assert_eq!(find_last_json_string(content, "model"), Some("claude-sonnet-4-20250514".to_string()));
        assert_eq!(find_last_json_string(content, "missing"), None);
    }

    #[test]
    fn test_parse_last_usage_complete() {
        let content = r#"{"type":"assistant","model":"claude-sonnet-4-20250514","cache_creation_input_tokens":298,"cache_read_input_tokens":153696,"input_tokens":1,"output_tokens":24}"#;
        let info = parse_last_usage(content).unwrap();
        assert_eq!(info.cache_creation, 298);
        assert_eq!(info.cache_read, 153696);
        assert_eq!(info.input, 1);
        assert_eq!(info.output, 24);
        assert_eq!(info.total_tokens, 154019);
        assert_eq!(info.window_size, 200_000);
        assert!((info.percent - 77.0).abs() < 0.1);
        assert_eq!(info.model, Some("claude-sonnet-4-20250514".to_string()));
    }

    #[test]
    fn test_parse_last_usage_missing_field() {
        let content = r#"{"cache_creation_input_tokens":298,"cache_read_input_tokens":153696}"#;
        assert!(parse_last_usage(content).is_none());
    }

    #[test]
    fn test_parse_last_usage_dynamic_window_upgrade() {
        // Tokens exceed 200K → window auto-upgrades to 1M
        let content = r#"{"cache_creation_input_tokens":1000,"cache_read_input_tokens":250000,"input_tokens":500,"output_tokens":10000}"#;
        let info = parse_last_usage(content).unwrap();
        assert_eq!(info.total_tokens, 261500);
        assert_eq!(info.window_size, 1_000_000);
        assert!((info.percent - 26.15).abs() < 0.01);
    }

    #[test]
    fn test_read_last_usage_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();

        // Write multiple lines — only the last should be parsed
        writeln!(f, r#"{{"type":"assistant","cache_creation_input_tokens":100,"cache_read_input_tokens":5000,"input_tokens":10,"output_tokens":5}}"#).unwrap();
        writeln!(f, r#"{{"type":"assistant","cache_creation_input_tokens":200,"cache_read_input_tokens":80000,"input_tokens":50,"output_tokens":1000}}"#).unwrap();

        let info = read_last_usage(&path).unwrap();
        assert_eq!(info.cache_creation, 200);
        assert_eq!(info.cache_read, 80000);
        assert_eq!(info.input, 50);
        assert_eq!(info.output, 1000);
        assert_eq!(info.total_tokens, 81250);
    }

    #[test]
    fn test_read_last_usage_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.jsonl");
        std::fs::File::create(&path).unwrap();

        assert!(read_last_usage(&path).is_none());
    }

    #[test]
    fn test_find_transcript_not_found() {
        // With a fake session_id, should return None
        assert!(find_transcript("nonexistent-session-id-12345").is_none());
    }
}
