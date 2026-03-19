use std::path::Path;

use crate::error::AppError;
use crate::model::SessionSource;

pub fn detect_format(path: &Path) -> Result<SessionSource, AppError> {
    if matches_claude_path(path) {
        return Ok(SessionSource::ClaudeCode);
    }

    if matches_copilot_path(path) {
        return Ok(SessionSource::CopilotCli);
    }

    if is_jsonl(path) {
        let content = std::fs::read_to_string(path).map_err(AppError::Io)?;
        return detect_format_from_content(&content);
    }

    Err(AppError::UnknownFormat(format!(
        "Cannot detect format for: {}",
        path.display()
    )))
}

pub fn detect_format_from_content(content: &str) -> Result<SessionSource, AppError> {
    let first_line = content.lines().next().unwrap_or("");

    let Ok(val) = serde_json::from_str::<serde_json::Value>(first_line) else {
        return Err(AppError::UnknownFormat(
            "Cannot parse first line as JSON".to_string(),
        ));
    };

    if is_claude_record(&val) {
        return Ok(SessionSource::ClaudeCode);
    }

    if is_copilot_record(&val) {
        return Ok(SessionSource::CopilotCli);
    }

    Err(AppError::UnknownFormat(
        "Cannot detect format from content".to_string(),
    ))
}

fn matches_claude_path(path: &Path) -> bool {
    path.to_string_lossy().contains(".claude")
}

fn matches_copilot_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    path_str.contains(".copilot") || path_str.contains("copilot")
}

fn is_jsonl(path: &Path) -> bool {
    path.extension().map_or(false, |ext| ext == "jsonl")
}

fn is_claude_record(val: &serde_json::Value) -> bool {
    let has_typed_message = val.get("type").is_some() && val.get("message").is_some();
    let is_summary = val.get("type").and_then(|t| t.as_str()) == Some("summary");
    let has_session_id = val.get("sessionId").is_some();
    (has_typed_message || is_summary) && has_session_id
}

fn is_copilot_record(val: &serde_json::Value) -> bool {
    let event_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let has_data = val.get("data").is_some();
    let has_id = val.get("id").is_some();
    event_type.starts_with("session.") && has_data && has_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_claude_from_content() {
        let content = r#"{"type":"user","message":{"content":"hello"},"sessionId":"abc-123"}"#;
        assert_eq!(
            detect_format_from_content(content).unwrap(),
            SessionSource::ClaudeCode
        );
    }

    #[test]
    fn test_detect_copilot_from_content() {
        let content = r#"{"type":"session.start","data":{"sessionId":"s1"},"id":"e1","timestamp":"2026-01-01T00:00:00Z","parentId":null}"#;
        assert_eq!(
            detect_format_from_content(content).unwrap(),
            SessionSource::CopilotCli
        );
    }

    #[test]
    fn test_detect_unknown_content() {
        let content = r#"{"foo":"bar"}"#;
        assert!(detect_format_from_content(content).is_err());
    }

    #[test]
    fn test_detect_claude_path() {
        let path = Path::new("/home/user/.claude/projects/test/abc.jsonl");
        assert_eq!(detect_format(path).unwrap(), SessionSource::ClaudeCode);
    }

    #[test]
    fn test_detect_copilot_path() {
        let path = Path::new("/home/user/.copilot/session-state/s1/events.jsonl");
        assert_eq!(detect_format(path).unwrap(), SessionSource::CopilotCli);
    }
}
