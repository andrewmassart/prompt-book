use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde_json::Value;

use crate::error::AppError;
use crate::model::{
    ContentBlock, Message, Role, Session, SessionMetadata, SessionSource,
};

use super::records::{CodexRecord, CodexSessionMeta, CodexTokenUsageInfo};
use super::{
    parse_mode, parse_tool_arguments, session_id_from_path,
    truncate_to_chars, ParseEvent, ParseState,
};

pub fn parse_codex_session(path: &Path) -> Result<Session, AppError> {
    let session_id = session_id_from_path(path);

    let mut session = super::parse_jsonl_session(
        path,
        SessionSource::Codex,
        session_id,
        |line, state| {
            let Ok(record) = serde_json::from_str::<CodexRecord>(line) else { return };
            process_codex_record(&record, state);
        },
    )?;

    if session.id.is_empty() {
        session.id = session_id_from_path(path);
    }

    if session.title.is_none() {
        session.title = title_from_session_index(&session.id);
    }

    Ok(session)
}

pub fn parse_codex_content(filename: &str, content: &str) -> Result<Session, AppError> {
    let path = std::path::Path::new(filename);
    let session_id = session_id_from_path(path);

    let mut session = super::parse_jsonl_from_content(
        content,
        SessionSource::Codex,
        session_id,
        path,
        |line, state| {
            let Ok(record) = serde_json::from_str::<CodexRecord>(line) else { return };
            process_codex_record(&record, state);
        },
    )?;

    if session.id.is_empty() {
        session.id = session_id_from_path(path);
    }
    if session.title.is_none() {
        session.title = title_from_session_index(&session.id);
    }

    Ok(session)
}

fn process_codex_record(record: &CodexRecord, state: &mut ParseState) {
    let timestamp = record.timestamp().map(String::from);

    match record {
        CodexRecord::SessionMeta { payload: Some(p), .. } => {
            let (id, ts, meta) = session_meta_to_parts(p);
            if !id.is_empty() {
                state.apply(ParseEvent::SetSessionId(id));
            }
            if let Some(started) = ts.or_else(|| timestamp.clone()) {
                state.apply(ParseEvent::SetStartedAt(started));
            }
            state.apply(ParseEvent::SetMetadata(meta));
        }

        CodexRecord::TurnContext { payload: Some(p), .. } => {
            if let Some(m) = &p.model {
                state.apply(ParseEvent::SetModel(m.clone()));
            }
            if let Some(mode_str) = p.collaboration_mode.as_ref().and_then(|cm| cm.mode.as_deref()) {
                state.apply(ParseEvent::SetMode(parse_mode(mode_str)));
            }
        }

        CodexRecord::EventMsg { payload: Some(p), .. } => {
            process_event_msg(p, &timestamp, state);
        }

        CodexRecord::ResponseItem { payload: Some(p), .. } => {
            process_response_item(p, state);
        }

        _ => {}
    }
}

fn process_event_msg(p: &Value, timestamp: &Option<String>, state: &mut ParseState) {
    match event_msg_type(p) {
        "task_started" => {
            if !state.turn_items.is_empty() {
                state.apply(ParseEvent::FlushTurn { timestamp: None });
            }
            state.apply(ParseEvent::SetTurnTimestamp(timestamp.clone()));
            if let Some(mode_str) = p.get("collaboration_mode_kind").and_then(Value::as_str) {
                state.apply(ParseEvent::SetMode(parse_mode(mode_str)));
            }
        }
        "user_message" => {
            let mut content: Vec<ContentBlock> = p
                .get("message")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(|text| ContentBlock::Text { text: text.to_string() })
                .into_iter()
                .collect();

            if let Some(images) = p.get("images").and_then(Value::as_array) {
                content.extend(images.iter().filter_map(extract_image_block));
            }

            if content.is_empty() {
                content.push(ContentBlock::Text { text: String::new() });
            }

            state.apply(ParseEvent::AddMessage(Message {
                id: uuid::Uuid::new_v4().to_string(),
                role: Role::User,
                timestamp: timestamp.clone(),
                content,
                mode: state.current_mode,
                is_agent: false,
                is_meta: false,
                duration_ms: None,
            }));
        }
        "task_complete" => {
            state.apply(ParseEvent::FlushTurn { timestamp: None });
        }
        "agent_message" => {
            if let Some(text) = p.get("message").and_then(Value::as_str).filter(|t| !t.is_empty()) {
                state.apply(ParseEvent::PushTurnItem(ContentBlock::Text {
                    text: text.to_string(),
                }));
            }
        }
        "token_count" => {
            if let Some(usage) = extract_token_usage(p) {
                state.apply(ParseEvent::MergeUsage {
                    input: usage.input_tokens.unwrap_or(0),
                    output: usage.output_tokens.unwrap_or(0),
                    cache_read: usage.cached_input_tokens.unwrap_or(0),
                    cache_write: 0,
                });
            }
        }
        _ => {}
    }
}

fn process_response_item(p: &Value, state: &mut ParseState) {
    match p.get("type").and_then(Value::as_str).unwrap_or("") {
        "message" => {
            for block in parse_response_message(p) {
                state.apply(ParseEvent::PushTurnItem(block));
            }
        }
        "reasoning" => {
            for block in parse_response_reasoning(p) {
                state.apply(ParseEvent::PushTurnItem(block));
            }
        }
        "function_call" => {
            let tool_name = str_field(p, "name").unwrap_or("unknown");
            let call_id = str_field(p, "call_id").unwrap_or("");
            state.apply(ParseEvent::PushTurnItem(ContentBlock::ToolUse {
                tool_name: format!("{tool_name}:{call_id}"),
                tool_call_id: Some(call_id.to_string()),
                input: parse_tool_arguments(p),
                output: None,
                duration_ms: None,
            }));
        }
        "function_call_output" => {
            let call_id = str_field(p, "call_id").unwrap_or("");
            let output = p.get("output").and_then(Value::as_str).map(String::from);
            state.apply(ParseEvent::AttachToolOutputBySuffix {
                suffix: format!(":{call_id}"),
                output,
            });
        }
        _ => {}
    }
}

fn extract_image_block(img: &Value) -> Option<ContentBlock> {
    let url = img
        .as_str()
        .or_else(|| img.get("url").and_then(Value::as_str))?;
    Some(ContentBlock::Image { source: url.to_string() })
}

fn str_field<'a>(val: &'a Value, key: &str) -> Option<&'a str> {
    val.get(key).and_then(Value::as_str)
}

fn session_meta_to_parts(meta: &CodexSessionMeta) -> (String, Option<String>, SessionMetadata) {
    let session_id = meta.id.clone().unwrap_or_default();
    let started_at = meta.timestamp.clone();
    let metadata = SessionMetadata {
        working_directory: meta.cwd.clone(),
        git_branch: meta.git.as_ref().and_then(|g| g.branch.clone()),
        repository: meta.git.as_ref().and_then(|g| g.repository_url.clone()),
        ..SessionMetadata::default()
    };
    (session_id, started_at, metadata)
}

fn event_msg_type(payload: &Value) -> &str {
    payload
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn extract_token_usage(payload: &Value) -> Option<CodexTokenUsageInfo> {
    let last = payload.get("info")?.get("last_token_usage")?;
    serde_json::from_value(last.clone()).ok()
}

fn parse_response_message(payload: &Value) -> Vec<ContentBlock> {
    match payload.get("role").and_then(Value::as_str) {
        Some("assistant") => {}
        _ => return Vec::new(),
    }
    payload
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|block| {
            match block.get("type").and_then(Value::as_str) {
                Some("output_text") => block
                    .get("text")
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                    .map(|text| ContentBlock::Text { text: text.to_string() }),
                _ => None,
            }
        })
        .collect()
}

fn parse_response_reasoning(payload: &Value) -> Vec<ContentBlock> {
    let summary_block = payload
        .get("summary")
        .and_then(Value::as_array)
        .map(|summary| {
            summary
                .iter()
                .filter_map(|item| {
                    item.get("text")
                        .and_then(Value::as_str)
                        .or_else(|| item.as_str())
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|text| !text.is_empty())
        .map(|text| ContentBlock::Thinking { text });

    let content_block = payload
        .get("content")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty() && payload.get("encrypted_content").is_none())
        .map(|text| ContentBlock::Thinking { text: text.to_string() });

    summary_block.into_iter().chain(content_block).collect()
}

fn title_from_session_index(session_id: &str) -> Option<String> {
    let index_path = dirs::home_dir()?.join(".codex").join("session_index.jsonl");
    let file = File::open(index_path).ok()?;
    let reader = BufReader::new(file);

    reader.lines().map_while(Result::ok).find_map(|line| {
        let record: Value = serde_json::from_str(&line).ok()?;
        match record.get("id").and_then(Value::as_str) {
            Some(id) if id == session_id => {
                record.get("thread_name").and_then(Value::as_str).map(String::from)
            }
            _ => None,
        }
    })
}

pub fn scan_codex_summary(path: &Path) -> super::ScanSummary {
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

        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };

        let event_type = record.get("type").and_then(Value::as_str).unwrap_or("");
        let payload = record.get("payload");

        match event_type {
            "session_meta" => {
                if let Some(p) = payload {
                    session_id = p.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                    started_at = p.get("timestamp").and_then(Value::as_str).map(String::from)
                        .or_else(|| record.get("timestamp")?.as_str().map(String::from));
                }
            }
            "turn_context" => {
                model = model.or_else(|| payload?.get("model")?.as_str().map(String::from));
            }
            "event_msg" => {
                match payload.and_then(|p| p.get("type")).and_then(Value::as_str).unwrap_or("") {
                    "user_message" => {
                        message_count += 1;
                        title = title.or_else(|| {
                            payload?.get("message")?.as_str()
                                .filter(|s| !s.trim().is_empty())
                                .map(|s| truncate_to_chars(s, 100))
                        });
                    }
                    "task_complete" => {
                        message_count += 1;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    title = title.or_else(|| title_from_session_index(&session_id));

    Ok((title, model, started_at, message_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MessageMode;
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

        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, Role::User);
        assert_eq!(session.messages[1].role, Role::Assistant);
        assert_eq!(session.messages[0].mode, MessageMode::Plan);

        assert_eq!(session.title, Some("Fix the login bug".to_string()));

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

        assert_eq!(session.messages.len(), 2);
        let assistant = &session.messages[1];
        assert_eq!(assistant.content.len(), 2);
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

        assert_eq!(session.messages.len(), 4);
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
        assert_eq!(count, 2);
    }
}
