use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::error::AppError;
use crate::model::{ContentBlock, Message, MessageMode, Role, Session, SessionSource};

use super::records::{ClaudeContentBlock, ClaudeRecord};
use super::{
    clean_command_xml, extract_title_from_text, session_id_from_path,
    ParseEvent, ParseState,
};

pub fn parse_claude_session(path: &Path) -> Result<Session, AppError> {
    let session_id = session_id_from_path(path);
    let sid = session_id.clone();
    let mut session = super::parse_jsonl_session(
        path,
        SessionSource::ClaudeCode,
        session_id,
        |line, state| {
            let Ok(record) = serde_json::from_str::<ClaudeRecord>(line) else { return };
            process_claude_record(&record, state, &sid);
        },
    )?;
    if session.title.is_none() {
        session.title = title_from_path(path);
    }
    Ok(session)
}

pub fn parse_claude_content(filename: &str, content: &str) -> Result<Session, AppError> {
    let path = std::path::Path::new(filename);
    let session_id = session_id_from_path(path);
    let sid = session_id.clone();
    let mut session = super::parse_jsonl_from_content(
        content,
        SessionSource::ClaudeCode,
        session_id,
        path,
        |line, state| {
            let Ok(record) = serde_json::from_str::<ClaudeRecord>(line) else { return };
            process_claude_record(&record, state, &sid);
        },
    )?;
    if session.title.is_none() {
        session.title = title_from_path(path);
    }
    Ok(session)
}

fn process_claude_record(record: &ClaudeRecord, state: &mut ParseState, session_id: &str) {
    match record.session_id() {
        Some(sid) if sid != session_id => return,
        _ => {}
    }

    match record {
        ClaudeRecord::User {
            message, timestamp, slug, git_branch, permission_mode,
            uuid, is_sidechain, is_meta, duration_ms, ..
        } => {
            if let Some(ts) = timestamp { state.apply(ParseEvent::SetStartedAt(ts.clone())); }
            state.metadata.slug = state.metadata.slug.take().or_else(|| slug.clone());
            state.metadata.git_branch = state.metadata.git_branch.take().or_else(|| git_branch.clone());

            let mode = match permission_mode.as_deref() {
                Some("plan") => MessageMode::Plan,
                Some("acceptEdits") => MessageMode::Auto,
                _ => MessageMode::Normal,
            };
            state.apply(ParseEvent::SetMode(mode));

            state.apply(ParseEvent::AddMessage(Message {
                id: uuid.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                role: Role::User,
                timestamp: timestamp.clone(),
                content: message.as_ref()
                    .and_then(|m| m.content.as_ref())
                    .map(extract_content_blocks)
                    .unwrap_or_default(),
                mode,
                is_agent: is_sidechain.unwrap_or(false),
                is_meta: is_meta.unwrap_or(false),
                duration_ms: *duration_ms,
            }));
        }
        ClaudeRecord::Assistant {
            message: msg_payload, timestamp, uuid,
            is_sidechain, is_meta, duration_ms, ..
        } => {
            let content = msg_payload.as_ref()
                .and_then(|msg| msg.content.as_ref())
                .map(|blocks| blocks.iter().filter_map(parse_assistant_content_block).collect())
                .unwrap_or_default();

            if let Some(msg) = msg_payload {
                if let Some(ref m) = msg.model { state.apply(ParseEvent::SetModel(m.clone())); }
                if let Some(ref u) = msg.usage {
                    state.apply(ParseEvent::MergeUsage {
                        input: u.input_tokens.unwrap_or(0),
                        output: u.output_tokens.unwrap_or(0),
                        cache_read: u.cache_read_input_tokens.unwrap_or(0),
                        cache_write: u.cache_creation_input_tokens.unwrap_or(0),
                    });
                }
            }

            state.apply(ParseEvent::AddMessage(Message {
                id: uuid.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                role: Role::Assistant,
                timestamp: timestamp.clone(),
                content,
                mode: state.current_mode,
                is_agent: is_sidechain.unwrap_or(false),
                is_meta: is_meta.unwrap_or(false),
                duration_ms: *duration_ms,
            }));
        }
        ClaudeRecord::ToolResult { tool_use_id: Some(ref id), content, duration_ms, .. }
            if !id.is_empty() =>
        {
            state.apply(ParseEvent::AttachToolOutputById {
                tool_call_id: id.clone(),
                output: extract_tool_result_output(content),
                duration_ms: *duration_ms,
            });
        }
        ClaudeRecord::Summary { summary: Some(s), is_compact_summary, .. }
            if !is_compact_summary.unwrap_or(false) =>
        {
            state.apply(ParseEvent::SetTitle(s.clone()));
        }
        ClaudeRecord::System { message, timestamp, cwd, working_directory, .. } => {
            state.metadata.working_directory = state.metadata.working_directory.take()
                .or_else(|| cwd.clone())
                .or_else(|| working_directory.clone());

            state.apply(ParseEvent::AddMessage(Message {
                id: uuid::Uuid::new_v4().to_string(),
                role: Role::System,
                timestamp: timestamp.clone(),
                content: message.as_ref()
                    .and_then(|m| m.content.as_ref())
                    .map(extract_content_blocks)
                    .unwrap_or_default(),
                mode: state.current_mode,
                is_agent: false,
                is_meta: false,
                duration_ms: None,
            }));
        }
        _ => {}
    }
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

fn clean_text(text: &str) -> String {
    clean_command_xml(text).unwrap_or_else(|| text.to_string())
}

fn parse_content_block(block: &serde_json::Value) -> Option<ContentBlock> {
    let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match block_type {
        "image" => parse_image_block(block.get("source")),
        _ => block.get("text")?.as_str().map(|t| ContentBlock::Text { text: clean_text(t) }),
    }
}

fn extract_content_blocks(msg_content: &serde_json::Value) -> Vec<ContentBlock> {
    if let Some(text) = msg_content.as_str() {
        return vec![ContentBlock::Text { text: clean_text(text) }];
    }
    msg_content.as_array()
        .map(|arr| arr.iter().filter_map(parse_content_block).collect())
        .unwrap_or_default()
}

fn parse_assistant_content_block(block: &ClaudeContentBlock) -> Option<ContentBlock> {
    match block {
        ClaudeContentBlock::Text { text } => {
            let t = text.as_deref()?;
            Some(ContentBlock::Text {
                text: t.to_string(),
            })
        }
        ClaudeContentBlock::ToolUse { id, name, input } => {
            Some(ContentBlock::ToolUse {
                tool_name: name.as_deref().unwrap_or("unknown").to_string(),
                tool_call_id: id.clone(),
                input: input.clone().unwrap_or(serde_json::Value::Null),
                output: None,
                duration_ms: None,
            })
        }
        ClaudeContentBlock::Thinking { thinking } => {
            let t = thinking.as_deref()?;
            if t.trim().is_empty() {
                return None;
            }
            Some(ContentBlock::Thinking {
                text: t.to_string(),
            })
        }
        ClaudeContentBlock::Image { source } => parse_image_block(source.as_ref()),
        ClaudeContentBlock::Unknown => None,
    }
}

fn parse_image_block(source: Option<&serde_json::Value>) -> Option<ContentBlock> {
    let source = source?;
    let source_type = source.get("type").and_then(|t| t.as_str());

    let image_source = match source_type {
        Some("base64") => {
            let media = source.get("media_type").and_then(|m| m.as_str()).unwrap_or("image/png");
            let data = source.get("data").and_then(|d| d.as_str())?;
            format!("data:{media};base64,{data}")
        }
        Some("url") => source.get("url").and_then(|u| u.as_str())?.to_string(),
        _ => source.as_str()
            .or_else(|| source.get("url").and_then(|u| u.as_str()))?
            .to_string(),
    };

    Some(ContentBlock::Image { source: image_source })
}

fn extract_tool_result_output(content: &Option<serde_json::Value>) -> Option<String> {
    let content = content.as_ref()?;
    match content {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(arr) => {
            let joined: String = arr.iter()
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n");
            Some(match joined.is_empty() {
                true => serde_json::to_string(content).unwrap_or_default(),
                false => joined,
            })
        }
        _ => Some(serde_json::to_string(content).unwrap_or_default()),
    }
}

/// Quickly scans a Claude Code session file for summary metadata without full parsing.
pub fn scan_claude_summary(path: &Path) -> super::ScanSummary {
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

        let record_type = record.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match record.get("sessionId").and_then(|s| s.as_str()) {
            Some(sid) if sid != session_id => continue,
            _ => {}
        }

        match record_type {
            "user" => {
                message_count += 1;
                started_at = started_at.or_else(|| record.get("timestamp")?.as_str().map(String::from));
                title = title.or_else(|| {
                    if record.get("isMeta").and_then(|v| v.as_bool()).unwrap_or(false) {
                        return None;
                    }
                    let content = record.get("message")?.get("content")?;
                    let text = content.as_str()
                        .or_else(|| content.as_array()?.first()?.get("text")?.as_str())?;
                    extract_title_from_text(text)
                });
            }
            "assistant" => {
                message_count += 1;
                model = model.or_else(|| {
                    record.get("message")?.get("model")?.as_str().map(String::from)
                });
            }
            "summary" if !record.get("isCompactSummary").and_then(|v| v.as_bool()).unwrap_or(false) => {
                title = title.or_else(|| record.get("summary")?.as_str().map(String::from));
            }
            _ => {}
        }
    }

    title = title.or_else(|| title_from_path(path));

    Ok((title, model, started_at, message_count))
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

    #[test]
    fn test_user_message_with_image() {
        let jsonl = r#"{"type":"user","message":{"content":[{"type":"text","text":"What is in this image?"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"iVBOR"}}]},"timestamp":"2025-01-01T00:00:01Z","sessionId":"img-test"}"#;

        let file = create_test_jsonl(jsonl);
        let path = copy_as_session(&file, "img-test");

        let session = parse_claude_session(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(session.messages[0].content.len(), 2);
        assert!(matches!(session.messages[0].content[0], ContentBlock::Text { .. }));
        if let ContentBlock::Image { source } = &session.messages[0].content[1] {
            assert_eq!(source, "data:image/png;base64,iVBOR");
        } else {
            panic!("Expected Image content block");
        }
    }
}
