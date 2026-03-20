use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::error::AppError;
use crate::model::{
    ContentBlock, Message, MessageMode, Role, Session, SessionMetadata, SessionSource, TokenUsage,
};

/// Parse a Codex CLI session JSONL file.
///
/// Real Codex format uses these top-level event types:
/// - `session_meta`     — session ID, cwd, git info, model_provider
/// - `event_msg`        — payload.type: task_started, user_message, task_complete,
///                         token_count, agent_message, thread_rolled_back
/// - `response_item`    — payload.type: message (role: user/assistant/developer),
///                         reasoning, function_call, function_call_output
/// - `turn_context`     — model, collaboration_mode, turn_id
pub fn parse_codex_session(path: &Path) -> Result<Session, AppError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut session_id = String::new();
    let mut started_at: Option<String> = None;
    let mut model: Option<String> = None;
    let mut messages: Vec<Message> = Vec::new();
    let mut usage_accumulator = UsageAccumulator::new();
    let mut current_mode = MessageMode::Normal;
    let mut metadata = SessionMetadata {
        working_directory: None,
        git_branch: None,
        slug: None,
        repository: None,
    };

    // Collect assistant content blocks between task_started and task_complete
    let mut turn_items: Vec<ContentBlock> = Vec::new();
    let mut turn_timestamp: Option<String> = None;
    let mut in_turn = false;

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
        let timestamp = extract_timestamp(&record);
        let payload = record.get("payload");

        match event_type {
            "session_meta" => {
                if let Some(p) = payload {
                    session_id = p
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    started_at = p
                        .get("timestamp")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .or(timestamp.clone());
                    metadata.working_directory = p
                        .get("cwd")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    if let Some(git) = p.get("git") {
                        metadata.git_branch = git
                            .get("branch")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                        metadata.repository = git
                            .get("repository_url")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                }
            }

            "turn_context" => {
                if let Some(p) = payload {
                    if model.is_none() {
                        model = p
                            .get("model")
                            .and_then(|m| m.as_str())
                            .map(String::from);
                    }
                    // Extract collaboration mode
                    if let Some(mode_str) = p
                        .get("collaboration_mode")
                        .and_then(|cm| cm.get("mode"))
                        .and_then(|m| m.as_str())
                    {
                        current_mode = match mode_str {
                            "plan" => MessageMode::Plan,
                            "auto" | "autopilot" | "full-auto" => MessageMode::Auto,
                            _ => MessageMode::Normal,
                        };
                    }
                }
            }

            "event_msg" => {
                let Some(p) = payload else { continue };
                let msg_type = p.get("type").and_then(|t| t.as_str()).unwrap_or("");

                match msg_type {
                    "task_started" => {
                        in_turn = true;
                        turn_items.clear();
                        turn_timestamp = timestamp.clone();
                        // Update mode from collaboration_mode_kind
                        if let Some(mode_str) = p
                            .get("collaboration_mode_kind")
                            .and_then(|m| m.as_str())
                        {
                            current_mode = match mode_str {
                                "plan" => MessageMode::Plan,
                                "auto" | "autopilot" | "full-auto" => MessageMode::Auto,
                                _ => MessageMode::Normal,
                            };
                        }
                    }
                    "user_message" => {
                        let mut content = Vec::new();
                        if let Some(text) = p.get("message").and_then(|m| m.as_str()) {
                            if !text.is_empty() {
                                content.push(ContentBlock::Text {
                                    text: text.to_string(),
                                });
                            }
                        }
                        // Handle images
                        if let Some(images) = p.get("images").and_then(|i| i.as_array()) {
                            for img in images {
                                if let Some(url) = img.as_str() {
                                    content.push(ContentBlock::Image {
                                        source: url.to_string(),
                                    });
                                } else if let Some(url) = img.get("url").and_then(|u| u.as_str()) {
                                    content.push(ContentBlock::Image {
                                        source: url.to_string(),
                                    });
                                }
                            }
                        }
                        if content.is_empty() {
                            content.push(ContentBlock::Text {
                                text: String::new(),
                            });
                        }
                        messages.push(Message {
                            id: uuid::Uuid::new_v4().to_string(),
                            role: Role::User,
                            timestamp: timestamp.clone(),
                            content,
                            mode: current_mode.clone(),
                            is_agent: false,
                            is_meta: false,
                            duration_ms: None,
                        });
                    }
                    "task_complete" => {
                        if in_turn && !turn_items.is_empty() {
                            messages.push(Message {
                                id: uuid::Uuid::new_v4().to_string(),
                                role: Role::Assistant,
                                timestamp: turn_timestamp.take(),
                                content: std::mem::take(&mut turn_items),
                                mode: current_mode.clone(),
                                is_agent: false,
                                is_meta: false,
                                duration_ms: None,
                            });
                        }
                        in_turn = false;
                    }
                    "agent_message" => {
                        if let Some(text) = p.get("message").and_then(|m| m.as_str()) {
                            if !text.is_empty() {
                                turn_items.push(ContentBlock::Text {
                                    text: text.to_string(),
                                });
                            }
                        }
                    }
                    "token_count" => {
                        if let Some(info) = p.get("info") {
                            if let Some(last) = info.get("last_token_usage") {
                                usage_accumulator.add(last);
                            }
                        }
                    }
                    _ => {}
                }
            }

            "response_item" => {
                let Some(p) = payload else { continue };
                let item_type = p.get("type").and_then(|t| t.as_str()).unwrap_or("");

                match item_type {
                    "message" => {
                        let role = p.get("role").and_then(|r| r.as_str()).unwrap_or("");
                        match role {
                            "assistant" => {
                                // Extract text from content array
                                if let Some(arr) = p.get("content").and_then(|c| c.as_array()) {
                                    for block in arr {
                                        let bt = block
                                            .get("type")
                                            .and_then(|t| t.as_str())
                                            .unwrap_or("");
                                        match bt {
                                            "output_text" => {
                                                if let Some(text) =
                                                    block.get("text").and_then(|t| t.as_str())
                                                {
                                                    if !text.is_empty() {
                                                        turn_items.push(ContentBlock::Text {
                                                            text: text.to_string(),
                                                        });
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            // user/developer messages are context replay, skip
                            _ => {}
                        }
                    }
                    "reasoning" => {
                        // summary is an array of summary strings
                        if let Some(summary) = p.get("summary").and_then(|s| s.as_array()) {
                            let texts: Vec<String> = summary
                                .iter()
                                .filter_map(|item| {
                                    item.get("text")
                                        .and_then(|t| t.as_str())
                                        .or_else(|| item.as_str())
                                        .map(String::from)
                                })
                                .collect();
                            let text = texts.join("\n");
                            if !text.is_empty() {
                                turn_items.push(ContentBlock::Thinking { text });
                            }
                        }
                        // Fallback: content field (may be encrypted/null)
                        if let Some(text) = p.get("content").and_then(|c| c.as_str()) {
                            if !text.is_empty()
                                && p.get("encrypted_content").is_none()
                            {
                                turn_items.push(ContentBlock::Thinking {
                                    text: text.to_string(),
                                });
                            }
                        }
                    }
                    "function_call" => {
                        let tool_name = p
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let call_id = p
                            .get("call_id")
                            .and_then(|c| c.as_str())
                            .unwrap_or("")
                            .to_string();
                        let input = parse_arguments(p);
                        turn_items.push(ContentBlock::ToolUse {
                            tool_name: format!("{}:{}", tool_name, call_id),
                            input,
                            output: None,
                            duration_ms: None,
                        });
                    }
                    "function_call_output" => {
                        let call_id = p
                            .get("call_id")
                            .and_then(|c| c.as_str())
                            .unwrap_or("");
                        let output = p
                            .get("output")
                            .and_then(|o| o.as_str())
                            .map(String::from);

                        // Find the matching function_call ToolUse and attach output
                        let suffix = format!(":{}", call_id);
                        let mut found = false;
                        for item in turn_items.iter_mut().rev() {
                            if let ContentBlock::ToolUse {
                                tool_name,
                                output: ref mut out,
                                ..
                            } = item
                            {
                                if tool_name.ends_with(&suffix) && out.is_none() {
                                    // Restore clean tool name
                                    *tool_name = tool_name
                                        .strip_suffix(&suffix)
                                        .unwrap_or(tool_name)
                                        .to_string();
                                    *out = output.clone();
                                    found = true;
                                    break;
                                }
                            }
                        }
                        // Also check already-flushed messages
                        if !found {
                            for msg in messages.iter_mut().rev() {
                                for item in msg.content.iter_mut().rev() {
                                    if let ContentBlock::ToolUse {
                                        tool_name,
                                        output: ref mut out,
                                        ..
                                    } = item
                                    {
                                        if tool_name.ends_with(&suffix) && out.is_none() {
                                            *tool_name = tool_name
                                                .strip_suffix(&suffix)
                                                .unwrap_or(tool_name)
                                                .to_string();
                                            *out = output.clone();
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            _ => {}
        }
    }

    // Flush any remaining turn items
    if !turn_items.is_empty() {
        messages.push(Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: Role::Assistant,
            timestamp: turn_timestamp.take(),
            content: std::mem::take(&mut turn_items),
            mode: current_mode.clone(),
            is_agent: false,
            is_meta: false,
            duration_ms: None,
        });
    }

    if session_id.is_empty() {
        session_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
    }

    let title = title_from_first_user_message(&messages)
        .or_else(|| title_from_session_index(&session_id));

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
        metadata,
    })
}

fn extract_timestamp(record: &serde_json::Value) -> Option<String> {
    record
        .get("timestamp")
        .and_then(|t| t.as_str())
        .map(String::from)
}

fn parse_arguments(payload: &serde_json::Value) -> serde_json::Value {
    let args = payload
        .get("arguments")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    if let Some(s) = args.as_str() {
        serde_json::from_str(s).unwrap_or(serde_json::Value::String(s.to_string()))
    } else {
        args
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

fn title_from_session_index(session_id: &str) -> Option<String> {
    let home = dirs::home_dir()?;
    let index_path = home.join(".codex").join("session_index.jsonl");
    let file = File::open(index_path).ok()?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line.ok()?;
        let Ok(record) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if record.get("id").and_then(|id| id.as_str()) == Some(session_id) {
            return record
                .get("thread_name")
                .and_then(|n| n.as_str())
                .map(String::from);
        }
    }
    None
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

pub fn scan_codex_summary(
    path: &Path,
) -> Result<(Option<String>, Option<String>, Option<String>, usize), AppError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut session_id = String::new();
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
        let payload = record.get("payload");

        match event_type {
            "session_meta" => {
                if let Some(p) = payload {
                    session_id = p
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    started_at = p
                        .get("timestamp")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .or_else(|| extract_timestamp(&record));
                }
            }
            "turn_context" => {
                if model.is_none() {
                    if let Some(p) = payload {
                        model = p
                            .get("model")
                            .and_then(|m| m.as_str())
                            .map(String::from);
                    }
                }
            }
            "event_msg" => {
                if let Some(p) = payload {
                    let msg_type = p.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match msg_type {
                        "user_message" => {
                            message_count += 1;
                            if title.is_none() {
                                title = p
                                    .get("message")
                                    .and_then(|m| m.as_str())
                                    .filter(|s| !s.trim().is_empty())
                                    .map(|s| s.chars().take(100).collect());
                            }
                        }
                        "task_complete" => {
                            message_count += 1;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    if title.is_none() {
        title = title_from_session_index(&session_id);
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
    fn test_parse_basic_session() {
        let jsonl = r#"{"timestamp":"2026-03-20T10:00:00Z","type":"session_meta","payload":{"id":"abc-123","timestamp":"2026-03-20T10:00:00Z","cwd":"/home/user/project","originator":"Codex Desktop","cli_version":"0.115.0","source":"vscode","model_provider":"openai","git":{"branch":"main","repository_url":"https://github.com/user/project","commit_hash":"abc"}}}
{"timestamp":"2026-03-20T10:00:01Z","type":"event_msg","payload":{"type":"task_started","turn_id":"turn-1","model_context_window":200000,"collaboration_mode_kind":"plan"}}
{"timestamp":"2026-03-20T10:00:01Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Fix the login bug"}]}}
{"timestamp":"2026-03-20T10:00:01Z","type":"turn_context","payload":{"model":"gpt-5.3-codex","turn_id":"turn-1","collaboration_mode":{"mode":"plan"},"cwd":"/home/user/project"}}
{"timestamp":"2026-03-20T10:00:01Z","type":"event_msg","payload":{"type":"user_message","message":"Fix the login bug","images":[],"local_images":[],"text_elements":[]}}
{"timestamp":"2026-03-20T10:00:02Z","type":"response_item","payload":{"type":"reasoning","summary":[],"content":null,"encrypted_content":"encrypted..."}}
{"timestamp":"2026-03-20T10:00:03Z","type":"event_msg","payload":{"type":"agent_message","message":"I'll look into the login module.","phase":"commentary"}}
{"timestamp":"2026-03-20T10:00:03Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"I'll look into the login module."}],"phase":"commentary"}}
{"timestamp":"2026-03-20T10:00:05Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":0,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0,"total_tokens":0},"last_token_usage":{"input_tokens":500,"cached_input_tokens":100,"output_tokens":200,"reasoning_output_tokens":50,"total_tokens":850},"model_context_window":200000},"rate_limits":null}}
{"timestamp":"2026-03-20T10:00:05Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","last_agent_message":null}}"#;

        let file = write_jsonl(jsonl);
        let session = parse_codex_session(file.path()).unwrap();

        assert_eq!(session.id, "abc-123");
        assert_eq!(session.source, SessionSource::Codex);
        assert_eq!(session.started_at, Some("2026-03-20T10:00:00Z".to_string()));
        assert_eq!(session.model, Some("gpt-5.3-codex".to_string()));
        assert_eq!(session.metadata.working_directory, Some("/home/user/project".to_string()));
        assert_eq!(session.metadata.git_branch, Some("main".to_string()));

        // Should have: 1 user message + 1 assistant message
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, Role::User);
        assert_eq!(session.messages[1].role, Role::Assistant);
        assert_eq!(session.messages[0].mode, MessageMode::Plan);

        // Title from user_message
        assert_eq!(session.title, Some("Fix the login bug".to_string()));

        // Token usage from token_count event
        let usage = session.token_usage.unwrap();
        assert_eq!(usage.input_tokens, 500);
        assert_eq!(usage.output_tokens, 200);
        assert_eq!(usage.cache_read_tokens, Some(100));
    }

    #[test]
    fn test_parse_function_calls() {
        let jsonl = r#"{"timestamp":"2026-03-20T10:00:00Z","type":"session_meta","payload":{"id":"fc-test","timestamp":"2026-03-20T10:00:00Z","cwd":"/tmp","originator":"Codex Desktop","cli_version":"0.115.0","source":"vscode","model_provider":"openai"}}
{"timestamp":"2026-03-20T10:00:01Z","type":"event_msg","payload":{"type":"task_started","turn_id":"turn-1","model_context_window":200000,"collaboration_mode_kind":"interactive"}}
{"timestamp":"2026-03-20T10:00:01Z","type":"event_msg","payload":{"type":"user_message","message":"Read config.json","images":[],"local_images":[],"text_elements":[]}}
{"timestamp":"2026-03-20T10:00:02Z","type":"response_item","payload":{"type":"function_call","name":"read_file","arguments":"{\"path\":\"config.json\"}","call_id":"call_abc123"}}
{"timestamp":"2026-03-20T10:00:03Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call_abc123","output":"{ \"key\": \"value\" }"}}
{"timestamp":"2026-03-20T10:00:04Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Here is the config."}],"phase":"commentary"}}
{"timestamp":"2026-03-20T10:00:05Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","last_agent_message":null}}"#;

        let file = write_jsonl(jsonl);
        let session = parse_codex_session(file.path()).unwrap();

        assert_eq!(session.messages.len(), 2); // user + assistant
        let assistant = &session.messages[1];
        assert_eq!(assistant.content.len(), 2); // tool_use + text
        if let ContentBlock::ToolUse {
            tool_name, output, input, ..
        } = &assistant.content[0]
        {
            assert_eq!(tool_name, "read_file");
            assert_eq!(input.get("path").unwrap().as_str().unwrap(), "config.json");
            assert_eq!(output, &Some("{ \"key\": \"value\" }".to_string()));
        } else {
            panic!("Expected ToolUse block");
        }
        assert!(matches!(assistant.content[1], ContentBlock::Text { .. }));
    }

    #[test]
    fn test_parse_reasoning_summary() {
        let jsonl = r#"{"timestamp":"2026-03-20T10:00:00Z","type":"session_meta","payload":{"id":"reason-test","timestamp":"2026-03-20T10:00:00Z","cwd":"/tmp","originator":"Codex Desktop","cli_version":"0.115.0","source":"vscode","model_provider":"openai"}}
{"timestamp":"2026-03-20T10:00:01Z","type":"event_msg","payload":{"type":"task_started","turn_id":"turn-1","model_context_window":200000,"collaboration_mode_kind":"interactive"}}
{"timestamp":"2026-03-20T10:00:01Z","type":"event_msg","payload":{"type":"user_message","message":"Explain this","images":[],"local_images":[],"text_elements":[]}}
{"timestamp":"2026-03-20T10:00:02Z","type":"response_item","payload":{"type":"reasoning","summary":[{"text":"Analyzing the code structure"}],"content":null,"encrypted_content":"enc..."}}
{"timestamp":"2026-03-20T10:00:03Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Here is my analysis."}],"phase":"commentary"}}
{"timestamp":"2026-03-20T10:00:04Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","last_agent_message":null}}"#;

        let file = write_jsonl(jsonl);
        let session = parse_codex_session(file.path()).unwrap();

        let assistant = &session.messages[1];
        assert_eq!(assistant.content.len(), 2);
        if let ContentBlock::Thinking { text } = &assistant.content[0] {
            assert_eq!(text, "Analyzing the code structure");
        } else {
            panic!("Expected Thinking block");
        }
    }

    #[test]
    fn test_multiple_turns_accumulate_usage() {
        let jsonl = r#"{"timestamp":"2026-03-20T10:00:00Z","type":"session_meta","payload":{"id":"usage-test","timestamp":"2026-03-20T10:00:00Z","cwd":"/tmp","originator":"Codex Desktop","cli_version":"0.115.0","source":"vscode","model_provider":"openai"}}
{"timestamp":"2026-03-20T10:00:01Z","type":"event_msg","payload":{"type":"task_started","turn_id":"turn-1","model_context_window":200000,"collaboration_mode_kind":"interactive"}}
{"timestamp":"2026-03-20T10:00:01Z","type":"event_msg","payload":{"type":"user_message","message":"First","images":[],"local_images":[],"text_elements":[]}}
{"timestamp":"2026-03-20T10:00:02Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Reply 1"}]}}
{"timestamp":"2026-03-20T10:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"cached_input_tokens":10,"output_tokens":50,"reasoning_output_tokens":0,"total_tokens":160}},"rate_limits":null}}
{"timestamp":"2026-03-20T10:00:03Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","last_agent_message":null}}
{"timestamp":"2026-03-20T10:01:00Z","type":"event_msg","payload":{"type":"task_started","turn_id":"turn-2","model_context_window":200000,"collaboration_mode_kind":"interactive"}}
{"timestamp":"2026-03-20T10:01:00Z","type":"event_msg","payload":{"type":"user_message","message":"Second","images":[],"local_images":[],"text_elements":[]}}
{"timestamp":"2026-03-20T10:01:01Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Reply 2"}]}}
{"timestamp":"2026-03-20T10:01:02Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":200,"cached_input_tokens":50,"output_tokens":80,"reasoning_output_tokens":0,"total_tokens":330}},"rate_limits":null}}
{"timestamp":"2026-03-20T10:01:02Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-2","last_agent_message":null}}"#;

        let file = write_jsonl(jsonl);
        let session = parse_codex_session(file.path()).unwrap();

        assert_eq!(session.messages.len(), 4); // 2 user + 2 assistant
        let usage = session.token_usage.unwrap();
        assert_eq!(usage.input_tokens, 300);
        assert_eq!(usage.output_tokens, 130);
        assert_eq!(usage.cache_read_tokens, Some(60));
    }

    #[test]
    fn test_scan_summary() {
        let jsonl = r#"{"timestamp":"2026-03-20T10:00:00Z","type":"session_meta","payload":{"id":"scan-test","timestamp":"2026-03-20T10:00:00Z","cwd":"/tmp","originator":"Codex Desktop","cli_version":"0.115.0","source":"vscode","model_provider":"openai"}}
{"timestamp":"2026-03-20T10:00:01Z","type":"turn_context","payload":{"model":"gpt-5.3-codex","turn_id":"turn-1","collaboration_mode":{"mode":"plan"},"cwd":"/tmp"}}
{"timestamp":"2026-03-20T10:00:01Z","type":"event_msg","payload":{"type":"user_message","message":"Build a REST API","images":[],"local_images":[],"text_elements":[]}}
{"timestamp":"2026-03-20T10:00:05Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","last_agent_message":null}}"#;

        let file = write_jsonl(jsonl);
        let (title, model, started_at, count) = scan_codex_summary(file.path()).unwrap();

        assert_eq!(title, Some("Build a REST API".to_string()));
        assert_eq!(model, Some("gpt-5.3-codex".to_string()));
        assert_eq!(started_at, Some("2026-03-20T10:00:00Z".to_string()));
        assert_eq!(count, 2); // 1 user_message + 1 task_complete
    }
}
