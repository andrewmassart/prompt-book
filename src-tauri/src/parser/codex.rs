use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::error::AppError;
use crate::model::{
    ContentBlock, Message, MessageMode, Role, Session, SessionMetadata, SessionSource, TokenUsage,
};

pub fn parse_codex_session(path: &Path) -> Result<Session, AppError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut session_id = String::new();
    let mut started_at: Option<String> = None;
    let mut model: Option<String> = None;
    let mut messages: Vec<Message> = Vec::new();
    let mut usage_accumulator = UsageAccumulator::new();
    let mut turn_items: Vec<ContentBlock> = Vec::new();
    let mut turn_timestamp: Option<String> = None;

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        let event_type = record.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match event_type {
            "thread.started" => {
                session_id = record
                    .get("thread_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                started_at = record
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .map(String::from);
            }
            "message" => {
                let role = record
                    .get("role")
                    .and_then(|r| r.as_str())
                    .unwrap_or("");
                if role == "user" {
                    let content = extract_user_content_blocks(&record);
                    messages.push(Message {
                        id: uuid::Uuid::new_v4().to_string(),
                        role: Role::User,
                        timestamp: extract_timestamp(&record),
                        content,
                        mode: MessageMode::Normal,
                        is_agent: false,
                        is_meta: false,
                        duration_ms: None,
                    });
                }
            }
            "turn.started" => {
                turn_items.clear();
                turn_timestamp = extract_timestamp(&record);
            }
            "item.completed" => {
                if let Some(item) = record.get("item") {
                    let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match item_type {
                        "agent_message" => {
                            if let Some(text) = extract_item_text(item) {
                                if !text.is_empty() {
                                    turn_items.push(ContentBlock::Text { text });
                                }
                            }
                            if model.is_none() {
                                model = item
                                    .get("model")
                                    .and_then(|m| m.as_str())
                                    .map(String::from);
                            }
                        }
                        "reasoning" => {
                            if let Some(text) = extract_item_text(item) {
                                if !text.is_empty() {
                                    turn_items.push(ContentBlock::Thinking { text });
                                }
                            }
                        }
                        "command_execution" => {
                            let tool_name = "command_execution".to_string();
                            let command = item
                                .get("command")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null);
                            let output = item
                                .get("output")
                                .and_then(|o| o.as_str())
                                .map(String::from);
                            turn_items.push(ContentBlock::ToolUse {
                                tool_name,
                                input: command,
                                output,
                                duration_ms: None,
                            });
                        }
                        "file_change" => {
                            let tool_name = "file_change".to_string();
                            let mut input = serde_json::Map::new();
                            if let Some(f) = item.get("file").and_then(|f| f.as_str()) {
                                input.insert(
                                    "file".to_string(),
                                    serde_json::Value::String(f.to_string()),
                                );
                            }
                            if let Some(a) = item.get("action").and_then(|a| a.as_str()) {
                                input.insert(
                                    "action".to_string(),
                                    serde_json::Value::String(a.to_string()),
                                );
                            }
                            let diff = item
                                .get("diff")
                                .and_then(|d| d.as_str())
                                .map(String::from);
                            turn_items.push(ContentBlock::ToolUse {
                                tool_name,
                                input: serde_json::Value::Object(input),
                                output: diff,
                                duration_ms: None,
                            });
                        }
                        "mcp_tool_call" => {
                            let tool_name = item
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("mcp_tool")
                                .to_string();
                            let input = item
                                .get("arguments")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null);
                            let output = item
                                .get("result")
                                .and_then(|r| r.as_str())
                                .map(String::from);
                            turn_items.push(ContentBlock::ToolUse {
                                tool_name,
                                input,
                                output,
                                duration_ms: None,
                            });
                        }
                        _ => {}
                    }
                }
            }
            "turn.completed" => {
                if !turn_items.is_empty() {
                    messages.push(Message {
                        id: uuid::Uuid::new_v4().to_string(),
                        role: Role::Assistant,
                        timestamp: turn_timestamp.take(),
                        content: std::mem::take(&mut turn_items),
                        mode: MessageMode::Normal,
                        is_agent: false,
                        is_meta: false,
                        duration_ms: None,
                    });
                }
                accumulate_turn_usage(&record, &mut usage_accumulator);
            }
            "turn.failed" | "error" => {
                // Flush any pending turn items first
                if !turn_items.is_empty() {
                    messages.push(Message {
                        id: uuid::Uuid::new_v4().to_string(),
                        role: Role::Assistant,
                        timestamp: turn_timestamp.take(),
                        content: std::mem::take(&mut turn_items),
                        mode: MessageMode::Normal,
                        is_agent: false,
                        is_meta: false,
                        duration_ms: None,
                    });
                }
                let error_text = record
                    .get("message")
                    .or_else(|| record.get("error"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown error")
                    .to_string();
                messages.push(Message {
                    id: uuid::Uuid::new_v4().to_string(),
                    role: Role::System,
                    timestamp: extract_timestamp(&record),
                    content: vec![ContentBlock::Text { text: error_text }],
                    mode: MessageMode::Normal,
                    is_agent: false,
                    is_meta: false,
                    duration_ms: None,
                });
            }
            _ => {}
        }
    }

    if session_id.is_empty() {
        session_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
    }

    let title = title_from_first_user_message(&messages);

    super::calculate_durations(&mut messages);

    Ok(Session {
        id: session_id,
        source: SessionSource::Codex,
        source_path: path.to_path_buf(),
        title,
        model,
        started_at,
        messages,
        token_usage: usage_accumulator.into_token_usage(),
        metadata: SessionMetadata {
            working_directory: None,
            git_branch: None,
            slug: None,
            repository: None,
        },
    })
}

fn extract_timestamp(record: &serde_json::Value) -> Option<String> {
    record
        .get("created_at")
        .or_else(|| record.get("timestamp"))
        .and_then(|t| t.as_str())
        .map(String::from)
}

fn extract_message_text(record: &serde_json::Value) -> String {
    // Try content array first, then content as string, then text field
    if let Some(content) = record.get("content") {
        if let Some(s) = content.as_str() {
            return s.to_string();
        }
        if let Some(arr) = content.as_array() {
            let texts: Vec<String> = arr
                .iter()
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .map(String::from)
                .collect();
            if !texts.is_empty() {
                return texts.join("\n");
            }
        }
    }
    record
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string()
}

fn extract_user_content_blocks(record: &serde_json::Value) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();

    if let Some(content) = record.get("content") {
        if let Some(s) = content.as_str() {
            blocks.push(ContentBlock::Text {
                text: s.to_string(),
            });
        } else if let Some(arr) = content.as_array() {
            // OpenAI format: [{"type":"text","text":"..."}, {"type":"image_url","image_url":{"url":"..."}}]
            for part in arr {
                let part_type = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match part_type {
                    "text" | "input_text" => {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            blocks.push(ContentBlock::Text {
                                text: text.to_string(),
                            });
                        }
                    }
                    "image_url" => {
                        if let Some(url) = part
                            .get("image_url")
                            .and_then(|i| i.get("url"))
                            .and_then(|u| u.as_str())
                        {
                            blocks.push(ContentBlock::Image {
                                source: url.to_string(),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if blocks.is_empty() {
        let text = extract_message_text(record);
        blocks.push(ContentBlock::Text { text });
    }

    blocks
}

fn extract_item_text(item: &serde_json::Value) -> Option<String> {
    // Try content array
    if let Some(arr) = item.get("content").and_then(|c| c.as_array()) {
        let texts: Vec<String> = arr
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .map(String::from)
            .collect();
        if !texts.is_empty() {
            return Some(texts.join("\n"));
        }
    }
    // Try text field directly
    item.get("text")
        .and_then(|t| t.as_str())
        .map(String::from)
}

struct UsageAccumulator {
    input_tokens: u64,
    output_tokens: u64,
    cached_input: u64,
    has_data: bool,
}

impl UsageAccumulator {
    fn new() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            cached_input: 0,
            has_data: false,
        }
    }

    fn add(&mut self, usage: &serde_json::Value) {
        self.has_data = true;
        self.input_tokens += usage
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        self.output_tokens += usage
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        self.cached_input += usage
            .get("cached_input_tokens")
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
            cache_read_tokens: if self.cached_input > 0 {
                Some(self.cached_input)
            } else {
                None
            },
            cache_write_tokens: None,
        })
    }
}

fn accumulate_turn_usage(record: &serde_json::Value, acc: &mut UsageAccumulator) {
    if let Some(usage) = record.get("usage") {
        acc.add(usage);
    }
}

fn title_from_first_user_message(messages: &[Message]) -> Option<String> {
    messages
        .iter()
        .find(|m| m.role == Role::User)
        .and_then(|m| m.content.first())
        .and_then(|c| match c {
            ContentBlock::Text { text } if !text.trim().is_empty() => {
                Some(text.chars().take(100).collect())
            }
            _ => None,
        })
}

pub fn scan_codex_summary(
    path: &Path,
) -> Result<(Option<String>, Option<String>, Option<String>, usize), AppError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

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

        let event_type = record.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match event_type {
            "thread.started" => {
                started_at = record
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .map(String::from);
            }
            "message" => {
                let role = record.get("role").and_then(|r| r.as_str()).unwrap_or("");
                if role == "user" {
                    message_count += 1;
                    if title.is_none() {
                        let text = extract_message_text(&record);
                        if !text.trim().is_empty() {
                            title = Some(text.chars().take(100).collect());
                        }
                    }
                }
            }
            "turn.completed" => {
                message_count += 1;
            }
            "item.completed" => {
                if model.is_none() {
                    if let Some(item) = record.get("item") {
                        model = item
                            .get("model")
                            .and_then(|m| m.as_str())
                            .map(String::from);
                    }
                }
            }
            _ => {}
        }
    }

    Ok((title, model, started_at, message_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_jsonl(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", content).unwrap();
        file
    }

    #[test]
    fn test_parse_basic_codex_session() {
        let jsonl = r#"{"type":"thread.started","thread_id":"th_abc","session_id":"sess_1","created_at":"2026-03-20T10:00:00Z"}
{"type":"message","role":"user","content":"Fix the login bug","created_at":"2026-03-20T10:00:01Z"}
{"type":"turn.started","created_at":"2026-03-20T10:00:02Z"}
{"type":"item.completed","item":{"type":"agent_message","content":[{"type":"text","text":"I'll fix the login bug."}],"model":"o4-mini"}}
{"type":"turn.completed","usage":{"input_tokens":100,"output_tokens":50}}"#;

        let file = write_jsonl(jsonl);
        let session = parse_codex_session(file.path()).unwrap();

        assert_eq!(session.id, "th_abc");
        assert_eq!(session.source, SessionSource::Codex);
        assert_eq!(session.started_at, Some("2026-03-20T10:00:00Z".to_string()));
        assert_eq!(session.title, Some("Fix the login bug".to_string()));
        assert_eq!(session.model, Some("o4-mini".to_string()));
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, Role::User);
        assert_eq!(session.messages[1].role, Role::Assistant);

        let usage = session.token_usage.unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
    }

    #[test]
    fn test_parse_tool_use_items() {
        let jsonl = r#"{"type":"thread.started","thread_id":"th_tools","session_id":"sess_1","created_at":"2026-03-20T10:00:00Z"}
{"type":"message","role":"user","content":"Read the config file","created_at":"2026-03-20T10:00:01Z"}
{"type":"turn.started","created_at":"2026-03-20T10:00:02Z"}
{"type":"item.completed","item":{"type":"command_execution","command":"cat config.json","output":"{ \"key\": \"value\" }"}}
{"type":"item.completed","item":{"type":"agent_message","content":[{"type":"text","text":"Here is the config."}],"model":"o4-mini"}}
{"type":"turn.completed","usage":{"input_tokens":200,"output_tokens":100}}"#;

        let file = write_jsonl(jsonl);
        let session = parse_codex_session(file.path()).unwrap();

        assert_eq!(session.messages.len(), 2); // user + assistant
        let assistant = &session.messages[1];
        assert_eq!(assistant.content.len(), 2);
        assert!(matches!(assistant.content[0], ContentBlock::ToolUse { .. }));
        assert!(matches!(assistant.content[1], ContentBlock::Text { .. }));
    }

    #[test]
    fn test_parse_file_change_item() {
        let jsonl = r#"{"type":"thread.started","thread_id":"th_fc","session_id":"sess_1","created_at":"2026-03-20T10:00:00Z"}
{"type":"turn.started","created_at":"2026-03-20T10:00:01Z"}
{"type":"item.completed","item":{"type":"file_change","file":"src/main.rs","action":"modify","diff":"@@ -1,3 +1,4 @@\n+use std::io;\n"}}
{"type":"turn.completed","usage":{"input_tokens":50,"output_tokens":25}}"#;

        let file = write_jsonl(jsonl);
        let session = parse_codex_session(file.path()).unwrap();

        assert_eq!(session.messages.len(), 1);
        if let ContentBlock::ToolUse {
            tool_name, output, input, ..
        } = &session.messages[0].content[0]
        {
            assert_eq!(tool_name, "file_change");
            assert_eq!(input.get("file").unwrap().as_str().unwrap(), "src/main.rs");
            assert!(output.is_some());
        } else {
            panic!("Expected ToolUse block for file_change");
        }
    }

    #[test]
    fn test_parse_reasoning_item() {
        let jsonl = r#"{"type":"thread.started","thread_id":"th_reason","session_id":"sess_1","created_at":"2026-03-20T10:00:00Z"}
{"type":"turn.started","created_at":"2026-03-20T10:00:01Z"}
{"type":"item.completed","item":{"type":"reasoning","text":"Let me think about this..."}}
{"type":"item.completed","item":{"type":"agent_message","content":[{"type":"text","text":"Here is my answer."}],"model":"o4-mini"}}
{"type":"turn.completed","usage":{"input_tokens":100,"output_tokens":50}}"#;

        let file = write_jsonl(jsonl);
        let session = parse_codex_session(file.path()).unwrap();

        assert_eq!(session.messages.len(), 1);
        let assistant = &session.messages[0];
        assert_eq!(assistant.content.len(), 2);
        assert!(matches!(assistant.content[0], ContentBlock::Thinking { .. }));
        assert!(matches!(assistant.content[1], ContentBlock::Text { .. }));
    }

    #[test]
    fn test_parse_error_event() {
        let jsonl = r#"{"type":"thread.started","thread_id":"th_err","session_id":"sess_1","created_at":"2026-03-20T10:00:00Z"}
{"type":"message","role":"user","content":"Do something","created_at":"2026-03-20T10:00:01Z"}
{"type":"turn.started","created_at":"2026-03-20T10:00:02Z"}
{"type":"turn.failed","message":"Rate limit exceeded","created_at":"2026-03-20T10:00:03Z"}"#;

        let file = write_jsonl(jsonl);
        let session = parse_codex_session(file.path()).unwrap();

        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[1].role, Role::System);
        if let ContentBlock::Text { text } = &session.messages[1].content[0] {
            assert_eq!(text, "Rate limit exceeded");
        }
    }

    #[test]
    fn test_parse_mcp_tool_call() {
        let jsonl = r#"{"type":"thread.started","thread_id":"th_mcp","session_id":"sess_1","created_at":"2026-03-20T10:00:00Z"}
{"type":"turn.started","created_at":"2026-03-20T10:00:01Z"}
{"type":"item.completed","item":{"type":"mcp_tool_call","name":"web_search","arguments":{"query":"rust error handling"},"result":"Found 10 results"}}
{"type":"turn.completed","usage":{"input_tokens":100,"output_tokens":50}}"#;

        let file = write_jsonl(jsonl);
        let session = parse_codex_session(file.path()).unwrap();

        assert_eq!(session.messages.len(), 1);
        if let ContentBlock::ToolUse {
            tool_name, output, ..
        } = &session.messages[0].content[0]
        {
            assert_eq!(tool_name, "web_search");
            assert_eq!(output, &Some("Found 10 results".to_string()));
        } else {
            panic!("Expected ToolUse block for mcp_tool_call");
        }
    }

    #[test]
    fn test_usage_accumulates_across_turns() {
        let jsonl = r#"{"type":"thread.started","thread_id":"th_usage","session_id":"sess_1","created_at":"2026-03-20T10:00:00Z"}
{"type":"message","role":"user","content":"First question","created_at":"2026-03-20T10:00:01Z"}
{"type":"turn.started","created_at":"2026-03-20T10:00:02Z"}
{"type":"item.completed","item":{"type":"agent_message","content":[{"type":"text","text":"Answer 1"}],"model":"o4-mini"}}
{"type":"turn.completed","usage":{"input_tokens":100,"output_tokens":50,"cached_input_tokens":10}}
{"type":"message","role":"user","content":"Follow up","created_at":"2026-03-20T10:00:10Z"}
{"type":"turn.started","created_at":"2026-03-20T10:00:11Z"}
{"type":"item.completed","item":{"type":"agent_message","content":[{"type":"text","text":"Answer 2"}],"model":"o4-mini"}}
{"type":"turn.completed","usage":{"input_tokens":200,"output_tokens":80,"cached_input_tokens":50}}"#;

        let file = write_jsonl(jsonl);
        let session = parse_codex_session(file.path()).unwrap();

        let usage = session.token_usage.unwrap();
        assert_eq!(usage.input_tokens, 300);
        assert_eq!(usage.output_tokens, 130);
        assert_eq!(usage.cache_read_tokens, Some(60));
        assert!(usage.cache_write_tokens.is_none());
    }

    #[test]
    fn test_scan_codex_summary() {
        let jsonl = r#"{"type":"thread.started","thread_id":"th_scan","session_id":"sess_1","created_at":"2026-03-20T10:00:00Z"}
{"type":"message","role":"user","content":"Build a REST API","created_at":"2026-03-20T10:00:01Z"}
{"type":"turn.started","created_at":"2026-03-20T10:00:02Z"}
{"type":"item.completed","item":{"type":"agent_message","content":[{"type":"text","text":"Building..."}],"model":"o4-mini"}}
{"type":"turn.completed","usage":{"input_tokens":100,"output_tokens":50}}"#;

        let file = write_jsonl(jsonl);
        let (title, model, started_at, count) = scan_codex_summary(file.path()).unwrap();

        assert_eq!(title, Some("Build a REST API".to_string()));
        assert_eq!(model, Some("o4-mini".to_string()));
        assert_eq!(started_at, Some("2026-03-20T10:00:00Z".to_string()));
        assert_eq!(count, 2); // 1 user message + 1 turn.completed
    }
}
