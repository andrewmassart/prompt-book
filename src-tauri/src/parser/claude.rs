use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::error::AppError;
use crate::model::{
    ContentBlock, Message, MessageMode, Role, Session, SessionMetadata, SessionSource, TokenUsage,
};

pub fn parse_claude_session(path: &Path) -> Result<Session, AppError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let session_id = session_id_from_path(path);

    let mut messages: Vec<Message> = Vec::new();
    let mut title: Option<String> = None;
    let mut model: Option<String> = None;
    let mut started_at: Option<String> = None;
    let mut current_mode = MessageMode::Normal;
    let mut usage_accumulator = UsageAccumulator::new();
    let mut metadata = SessionMetadata {
        working_directory: None,
        git_branch: None,
        slug: None,
        repository: None,
    };

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        if !belongs_to_session(&record, &session_id) {
            continue;
        }

        let record_type = record.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match record_type {
            "user" => {
                if started_at.is_none() {
                    started_at = extract_timestamp(&record);
                }
                if extract_slug(&record).is_some() && metadata.slug.is_none() {
                    metadata.slug = extract_slug(&record);
                }
                if extract_git_branch(&record).is_some() && metadata.git_branch.is_none() {
                    metadata.git_branch = extract_git_branch(&record);
                }
                current_mode = extract_permission_mode(&record);
                let msg = build_message_with_mode(Role::User, &record,
                    record.get("message").and_then(|m| m.get("content")).map(extract_content_blocks).unwrap_or_default(),
                    &current_mode);
                messages.push(msg);
            }
            "assistant" => {
                let (msg, msg_model) = parse_assistant_message_with_mode(&record, &current_mode);
                if model.is_none() {
                    model = msg_model;
                }
                usage_accumulator.add_from_record(&record);
                messages.push(msg);
            }
            "tool_result" => {
                attach_tool_result(&mut messages, &record);
            }
            "summary" => {
                if !is_compact_summary(&record) {
                    if title.is_none() {
                        title = extract_summary_title(&record);
                    }
                }
            }
            "system" => {
                let msg = build_message_with_mode(Role::System, &record,
                    record.get("message").and_then(|m| m.get("content")).map(extract_content_blocks).unwrap_or_default(),
                    &current_mode);
                extract_metadata(&record, &mut metadata);
                messages.push(msg);
            }
            _ => {}
        }
    }

    if title.is_none() {
        title = title_from_first_user_message(&messages);
    }
    if title.is_none() {
        title = title_from_path(path);
    }

    super::calculate_durations(&mut messages);

    Ok(Session {
        id: session_id,
        source: SessionSource::ClaudeCode,
        source_path: path.to_path_buf(),
        title,
        model,
        started_at,
        messages,
        token_usage: usage_accumulator.into_token_usage(),
        metadata,
    })
}

fn session_id_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn belongs_to_session(record: &serde_json::Value, session_id: &str) -> bool {
    match record.get("sessionId").and_then(|s| s.as_str()) {
        Some(id) => id == session_id,
        None => true,
    }
}

fn extract_timestamp(record: &serde_json::Value) -> Option<String> {
    record
        .get("timestamp")
        .and_then(|t| t.as_str())
        .map(String::from)
}

fn extract_slug(record: &serde_json::Value) -> Option<String> {
    record.get("slug").and_then(|s| s.as_str()).map(String::from)
}

fn extract_git_branch(record: &serde_json::Value) -> Option<String> {
    record
        .get("gitBranch")
        .and_then(|s| s.as_str())
        .map(String::from)
}

fn extract_permission_mode(record: &serde_json::Value) -> MessageMode {
    match record.get("permissionMode").and_then(|p| p.as_str()) {
        Some("plan") => MessageMode::Plan,
        Some("acceptEdits") => MessageMode::Auto,
        _ => MessageMode::Normal,
    }
}

fn is_compact_summary(record: &serde_json::Value) -> bool {
    record
        .get("isCompactSummary")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn extract_summary_title(record: &serde_json::Value) -> Option<String> {
    record
        .get("summary")
        .and_then(|s| s.as_str())
        .map(String::from)
}

fn extract_metadata(record: &serde_json::Value, metadata: &mut SessionMetadata) {
    if let Some(cwd) = record
        .get("cwd")
        .or_else(|| record.get("workingDirectory"))
        .and_then(|v| v.as_str())
    {
        metadata.working_directory = Some(cwd.to_string());
    }
}

fn title_from_first_user_message(messages: &[Message]) -> Option<String> {
    messages
        .iter()
        .filter(|m| m.role == Role::User && !m.is_meta)
        .filter_map(|m| {
            m.content.iter().find_map(|c| match c {
                ContentBlock::Text { text } if is_meaningful_title(text) => {
                    Some(truncate_to_chars(text.trim(), 100))
                }
                _ => None,
            })
        })
        .next()
}

fn is_meaningful_title(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.len() < 5 {
        return false;
    }
    let noise_prefixes = [
        "[Request interrupted",
        "<local-command",
        "<system-reminder",
        "<caveat",
        "Set model to",
    ];
    !noise_prefixes.iter().any(|p| trimmed.starts_with(p))
}

fn title_from_path(path: &Path) -> Option<String> {
    let path_str = path.to_string_lossy();
    if path_str.contains(".claude/projects/") || path_str.contains(".claude\\projects\\") {
        let project_part = path_str
            .split(".claude/projects/")
            .chain(path_str.split(".claude\\projects\\"))
            .nth(1)?;
        let dir = project_part.split('/').next()
            .or_else(|| project_part.split('\\').next())?;
        let cleaned = dir
            .trim_start_matches("P--")
            .replace("--", "/")
            .replace('-', " ");
        return Some(cleaned);
    }
    None
}

struct UsageAccumulator {
    input_tokens: u64,
    output_tokens: u64,
    cache_read: u64,
    cache_write: u64,
    has_data: bool,
}

impl UsageAccumulator {
    fn new() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            cache_read: 0,
            cache_write: 0,
            has_data: false,
        }
    }

    fn add_from_record(&mut self, record: &serde_json::Value) {
        let Some(usage) = record
            .get("message")
            .and_then(|m| m.get("usage"))
        else {
            return;
        };
        self.has_data = true;
        self.input_tokens += usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        self.output_tokens += usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        self.cache_read += usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        self.cache_write += usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
    }

    fn into_token_usage(self) -> Option<TokenUsage> {
        if !self.has_data {
            return None;
        }
        Some(TokenUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cache_read_tokens: nonzero(self.cache_read),
            cache_write_tokens: nonzero(self.cache_write),
        })
    }
}

fn nonzero(val: u64) -> Option<u64> {
    if val > 0 { Some(val) } else { None }
}

fn extract_content_blocks(msg_content: &serde_json::Value) -> Vec<ContentBlock> {
    let mut content = Vec::new();
    if let Some(text) = msg_content.as_str() {
        content.push(ContentBlock::Text {
            text: text.to_string(),
        });
    } else if let Some(arr) = msg_content.as_array() {
        for block in arr {
            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                content.push(ContentBlock::Text {
                    text: text.to_string(),
                });
            }
        }
    }
    content
}

fn build_message_with_mode(
    role: Role,
    record: &serde_json::Value,
    content: Vec<ContentBlock>,
    mode: &MessageMode,
) -> Message {
    Message {
        id: record
            .get("uuid")
            .and_then(|u| u.as_str())
            .map(String::from)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        role,
        timestamp: extract_timestamp(record),
        content,
        mode: mode.clone(),
        is_agent: record
            .get("isSidechain")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        is_meta: record
            .get("isMeta")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        duration_ms: record
            .get("durationMs")
            .and_then(|v| v.as_u64()),
    }
}

fn parse_assistant_message_with_mode(record: &serde_json::Value, mode: &MessageMode) -> (Message, Option<String>) {
    let mut content = Vec::new();
    let mut msg_model = None;

    if let Some(msg) = record.get("message") {
        msg_model = msg.get("model").and_then(|m| m.as_str()).map(String::from);

        if let Some(arr) = msg.get("content").and_then(|c| c.as_array()) {
            for block in arr {
                if let Some(cb) = parse_assistant_content_block(block) {
                    content.push(cb);
                }
            }
        }
    }

    (build_message_with_mode(Role::Assistant, record, content, mode), msg_model)
}

fn parse_assistant_content_block(block: &serde_json::Value) -> Option<ContentBlock> {
    let block_type = block.get("type").and_then(|t| t.as_str())?;
    match block_type {
        "text" => {
            let text = block.get("text").and_then(|t| t.as_str())?;
            Some(ContentBlock::Text {
                text: text.to_string(),
            })
        }
        "tool_use" => {
            let tool_name = block
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string();
            let input = block
                .get("input")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            Some(ContentBlock::ToolUse {
                tool_name,
                input,
                output: None,
                duration_ms: None,
            })
        }
        "thinking" => {
            let text = block.get("thinking").and_then(|t| t.as_str())?;
            if text.trim().is_empty() {
                return None;
            }
            Some(ContentBlock::Thinking {
                text: text.to_string(),
            })
        }
        _ => None,
    }
}

fn extract_tool_result_output(record: &serde_json::Value) -> Option<String> {
    let content = record.get("content")?;
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }
    if let Some(arr) = content.as_array() {
        let texts: Vec<String> = arr
            .iter()
            .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
            .map(String::from)
            .collect();
        if texts.is_empty() {
            return Some(serde_json::to_string(content).unwrap_or_default());
        }
        return Some(texts.join("\n"));
    }
    Some(serde_json::to_string(content).unwrap_or_default())
}

fn attach_tool_result(messages: &mut [Message], record: &serde_json::Value) {
    let tool_use_id = record
        .get("tool_use_id")
        .and_then(|id| id.as_str())
        .unwrap_or("");

    if tool_use_id.is_empty() {
        return;
    }

    let output = extract_tool_result_output(record);
    let duration_ms = record.get("duration_ms").and_then(|d| d.as_u64());

    if let Some(block) = find_unresolved_tool_use(messages) {
        if let ContentBlock::ToolUse {
            output: ref mut out,
            duration_ms: ref mut dur,
            ..
        } = block
        {
            *out = output;
            *dur = duration_ms;
        }
    }
}

fn find_unresolved_tool_use(messages: &mut [Message]) -> Option<&mut ContentBlock> {
    for msg in messages.iter_mut().rev() {
        for block in msg.content.iter_mut() {
            if matches!(block, ContentBlock::ToolUse { output: None, .. }) {
                return Some(block);
            }
        }
    }
    None
}


fn truncate_to_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

pub fn scan_claude_summary(
    path: &Path,
) -> Result<(Option<String>, Option<String>, Option<String>, usize), AppError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let session_id = session_id_from_path(path);

    let mut title: Option<String> = None;
    let mut model: Option<String> = None;
    let mut started_at: Option<String> = None;
    let mut message_count: usize = 0;

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        if !belongs_to_session(&record, &session_id) {
            continue;
        }

        let record_type = record.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match record_type {
            "user" => {
                message_count += 1;
                if started_at.is_none() {
                    started_at = extract_timestamp(&record);
                }
                if title.is_none() {
                    title = extract_first_text_content(&record);
                }
            }
            "assistant" => {
                message_count += 1;
                if model.is_none() {
                    model = extract_model(&record);
                }
            }
            "summary" => {
                if !is_compact_summary(&record) {
                    if let Some(s) = extract_summary_title(&record) {
                        title = Some(s);
                    }
                }
            }
            _ => {}
        }
    }

    if title.is_none() {
        title = title_from_path(path);
    }

    Ok((title, model, started_at, message_count))
}

fn extract_first_text_content(record: &serde_json::Value) -> Option<String> {
    if record.get("isMeta").and_then(|v| v.as_bool()).unwrap_or(false) {
        return None;
    }
    let content = record.get("message")?.get("content")?;
    let text = if let Some(s) = content.as_str() {
        s.to_string()
    } else {
        content.as_array()?.first()?.get("text")?.as_str()?.to_string()
    };
    if is_meaningful_title(&text) {
        Some(truncate_to_chars(text.trim(), 100))
    } else {
        None
    }
}

fn extract_model(record: &serde_json::Value) -> Option<String> {
    record
        .get("message")?
        .get("model")?
        .as_str()
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_jsonl(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", content).unwrap();
        file
    }

    fn copy_as_session(file: &NamedTempFile, session_id: &str) -> std::path::PathBuf {
        let dir = file.path().parent().unwrap();
        let new_path = dir.join(format!("{session_id}.jsonl"));
        std::fs::copy(file.path(), &new_path).unwrap();
        new_path
    }

    #[test]
    fn test_parse_basic_session() {
        let jsonl = r#"{"type":"system","message":{"content":"System init"},"timestamp":"2025-01-01T00:00:00Z","sessionId":"test-session"}
{"type":"user","message":{"content":"Hello Claude"},"timestamp":"2025-01-01T00:00:01Z","sessionId":"test-session"}
{"type":"assistant","message":{"role":"assistant","model":"claude-sonnet-4-20250514","content":[{"type":"text","text":"Hello! How can I help?"}]},"timestamp":"2025-01-01T00:00:02Z","sessionId":"test-session"}
{"type":"summary","summary":"Test conversation","sessionId":"test-session"}"#;

        let file = create_test_jsonl(jsonl);
        let path = copy_as_session(&file, "test-session");

        let session = parse_claude_session(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(session.id, "test-session");
        assert_eq!(session.title, Some("Test conversation".to_string()));
        assert_eq!(session.model, Some("claude-sonnet-4-20250514".to_string()));
        assert_eq!(session.messages.len(), 3);
        assert_eq!(session.messages[1].role, Role::User);
        assert_eq!(session.messages[2].role, Role::Assistant);
    }

    #[test]
    fn test_parse_tool_use_with_result() {
        let jsonl = r#"{"type":"user","message":{"content":"Read file.txt"},"timestamp":"2025-01-01T00:00:01Z","sessionId":"tool-test"}
{"type":"assistant","message":{"role":"assistant","model":"claude-sonnet-4-20250514","content":[{"type":"tool_use","id":"tool_1","name":"Read","input":{"file_path":"file.txt"}}]},"timestamp":"2025-01-01T00:00:02Z","sessionId":"tool-test"}
{"type":"tool_result","tool_use_id":"tool_1","content":"file contents here","sessionId":"tool-test"}"#;

        let file = create_test_jsonl(jsonl);
        let path = copy_as_session(&file, "tool-test");

        let session = parse_claude_session(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(session.messages.len(), 2);
        if let ContentBlock::ToolUse { output, .. } = &session.messages[1].content[0] {
            assert_eq!(output, &Some("file contents here".to_string()));
        } else {
            panic!("Expected ToolUse content block");
        }
    }

    #[test]
    fn test_skips_parent_session_records() {
        let jsonl = r#"{"type":"user","message":{"content":"From parent"},"timestamp":"2025-01-01T00:00:01Z","sessionId":"parent-id"}
{"type":"user","message":{"content":"From this session"},"timestamp":"2025-01-01T00:00:02Z","sessionId":"child-id"}"#;

        let file = create_test_jsonl(jsonl);
        let path = copy_as_session(&file, "child-id");

        let session = parse_claude_session(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(session.messages.len(), 1);
        if let ContentBlock::Text { text } = &session.messages[0].content[0] {
            assert_eq!(text, "From this session");
        }
    }

    #[test]
    fn test_mode_propagates_to_assistant() {
        let jsonl = r#"{"type":"user","message":{"content":"Plan something"},"timestamp":"2025-01-01T00:00:01Z","sessionId":"mode-test","permissionMode":"plan"}
{"type":"assistant","message":{"role":"assistant","model":"claude-sonnet-4-20250514","content":[{"type":"text","text":"Planning..."}]},"timestamp":"2025-01-01T00:00:02Z","sessionId":"mode-test"}"#;

        let file = create_test_jsonl(jsonl);
        let path = copy_as_session(&file, "mode-test");

        let session = parse_claude_session(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(session.messages[0].mode, MessageMode::Plan);
        assert_eq!(session.messages[1].mode, MessageMode::Plan);
    }

    #[test]
    fn test_mode_resets_on_new_user_message() {
        let jsonl = r#"{"type":"user","message":{"content":"Plan this"},"timestamp":"2025-01-01T00:00:01Z","sessionId":"mode-reset","permissionMode":"plan"}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"ok"}]},"timestamp":"2025-01-01T00:00:02Z","sessionId":"mode-reset"}
{"type":"user","message":{"content":"Now do it"},"timestamp":"2025-01-01T00:00:03Z","sessionId":"mode-reset","permissionMode":"acceptEdits"}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"doing"}]},"timestamp":"2025-01-01T00:00:04Z","sessionId":"mode-reset"}"#;

        let file = create_test_jsonl(jsonl);
        let path = copy_as_session(&file, "mode-reset");

        let session = parse_claude_session(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(session.messages[0].mode, MessageMode::Plan);
        assert_eq!(session.messages[1].mode, MessageMode::Plan);
        assert_eq!(session.messages[2].mode, MessageMode::Auto);
        assert_eq!(session.messages[3].mode, MessageMode::Auto);
    }

    #[test]
    fn test_meta_messages_flagged() {
        let jsonl = r#"{"type":"user","message":{"content":"meta content"},"timestamp":"2025-01-01T00:00:01Z","sessionId":"meta-test","isMeta":true}
{"type":"user","message":{"content":"real content"},"timestamp":"2025-01-01T00:00:02Z","sessionId":"meta-test"}"#;

        let file = create_test_jsonl(jsonl);
        let path = copy_as_session(&file, "meta-test");

        let session = parse_claude_session(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert!(session.messages[0].is_meta);
        assert!(!session.messages[1].is_meta);
        assert_eq!(session.title, Some("real content".to_string()));
    }

    #[test]
    fn test_noise_titles_skipped() {
        let jsonl = r#"{"type":"user","message":{"content":"[Request interrupted by user for tool use]"},"timestamp":"2025-01-01T00:00:01Z","sessionId":"noise-test"}
{"type":"user","message":{"content":"<local-command-stdout>set model</local-command-stdout>"},"timestamp":"2025-01-01T00:00:02Z","sessionId":"noise-test"}
{"type":"user","message":{"content":"Implement the auth module"},"timestamp":"2025-01-01T00:00:03Z","sessionId":"noise-test"}"#;

        let file = create_test_jsonl(jsonl);
        let path = copy_as_session(&file, "noise-test");

        let session = parse_claude_session(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(session.title, Some("Implement the auth module".to_string()));
    }

    #[test]
    fn test_empty_thinking_blocks_filtered() {
        let jsonl = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":""},{"type":"text","text":"Hello"}]},"timestamp":"2025-01-01T00:00:01Z","sessionId":"think-test"}"#;

        let file = create_test_jsonl(jsonl);
        let path = copy_as_session(&file, "think-test");

        let session = parse_claude_session(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(session.messages[0].content.len(), 1);
        assert!(matches!(session.messages[0].content[0], ContentBlock::Text { .. }));
    }
}
