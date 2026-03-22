use std::path::Path;

use crate::error::AppError;
use crate::model::SessionSource;

/// Detects the session format from file path and content.
pub fn detect_format(path: &Path) -> Result<SessionSource, AppError> {
    if let Some(source) = detect_from_path_components(path) {
        return Ok(source);
    }
    match path.extension() {
        Some(ext) if ext == "jsonl" => {
            let content = std::fs::read_to_string(path)?;
            detect_format_from_content(&content)
        }
        _ => Err(AppError::UnknownFormat(format!("Cannot detect format for: {}", path.display()))),
    }
}

fn detect_from_path_components(path: &Path) -> Option<SessionSource> {
    path.ancestors()
        .filter_map(|p| p.file_name())
        .find_map(|name| match name.to_str()? {
            ".codex" | "codex" => Some(SessionSource::Codex),
            ".claude" => Some(SessionSource::ClaudeCode),
            ".copilot" | "copilot" => Some(SessionSource::CopilotCli),
            _ => None,
        })
}

/// Detects the session format by inspecting the first line of JSONL content.
pub fn detect_format_from_content(content: &str) -> Result<SessionSource, AppError> {
    let first_line = content.lines().next().unwrap_or("");
    let val = serde_json::from_str::<serde_json::Value>(first_line)
        .map_err(|_| AppError::UnknownFormat("Cannot parse first line as JSON".to_string()))?;

    let record_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match () {
        _ if is_claude_record(record_type, &val) => Ok(SessionSource::ClaudeCode),
        _ if is_copilot_record(record_type, &val) => Ok(SessionSource::CopilotCli),
        _ if is_codex_record(record_type, &val) => Ok(SessionSource::Codex),
        _ => Err(AppError::UnknownFormat("Cannot detect format from content".to_string())),
    }
}

fn is_claude_record(record_type: &str, val: &serde_json::Value) -> bool {
    val.get("sessionId").is_some()
        && (val.get("message").is_some() || record_type == "summary")
}

fn is_copilot_record(record_type: &str, val: &serde_json::Value) -> bool {
    record_type.starts_with("session.") && val.get("data").is_some() && val.get("id").is_some()
}

fn is_codex_record(record_type: &str, val: &serde_json::Value) -> bool {
    record_type == "session_meta" && val.get("payload").is_some()
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

    #[test]
    fn test_detect_codex_path() {
        let path = Path::new("/home/user/.codex/sessions/2026/03/20/rollout-abc.jsonl");
        assert_eq!(detect_format(path).unwrap(), SessionSource::Codex);
    }

    #[test]
    fn test_detect_codex_from_content() {
        let content = r#"{"timestamp":"2026-03-20T10:00:00Z","type":"session_meta","payload":{"id":"abc-123","timestamp":"2026-03-20T10:00:00Z","cwd":"/tmp","originator":"Codex Desktop","cli_version":"0.115.0","source":"vscode","model_provider":"openai"}}"#;
        assert_eq!(
            detect_format_from_content(content).unwrap(),
            SessionSource::Codex
        );
    }
}
