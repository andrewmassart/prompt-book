use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::error::AppError;
use crate::model::{
    ContentBlock, Message, MessageMode, Role, Session, SessionMetadata, SessionSource, TokenUsage,
};

pub fn parse_copilot_session(path: &Path) -> Result<Session, AppError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut session_id = String::new();
    let mut started_at: Option<String> = None;
    let mut model: Option<String> = None;
    let mut messages: Vec<Message> = Vec::new();
    let mut metadata = SessionMetadata {
        working_directory: None,
        git_branch: None,
        slug: None,
        repository: None,
    };
    let mut token_usage: Option<TokenUsage> = None;
    let mut current_mode = MessageMode::Normal;
    let mut tool_starts: HashMap<String, ToolStartInfo> = HashMap::new();
    let mut active_agents: HashMap<String, String> = HashMap::new();

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
        let data = record.get("data");
        let timestamp = extract_timestamp(&record);

        match event_type {
            "session.start" => {
                parse_session_start(data, &mut session_id, &mut started_at, &mut metadata);
            }
            "session.mode_changed" => {
                current_mode = parse_mode_change(data);
            }
            "session.shutdown" => {
                token_usage = parse_shutdown_usage(data);
                if model.is_none() {
                    model = extract_current_model(data);
                }
            }
            "session.warning" | "session.error" => {
                let msg = parse_session_event(data, timestamp, &current_mode);
                messages.push(msg);
            }
            "user.message" => {
                let msg = parse_user_message(data, timestamp, &current_mode);
                messages.push(msg);
            }
            "assistant.message" => {
                let (msg, tool_requests) =
                    parse_assistant_message(data, timestamp, &current_mode, &active_agents);
                if model.is_none() {
                    model = extract_message_model(data);
                }
                messages.push(msg);
                for (call_id, tool_name, _input) in tool_requests {
                    tool_starts.insert(call_id, ToolStartInfo { tool_name });
                }
            }
            "tool.execution_complete" => {
                attach_tool_completion(data, &tool_starts, &mut messages);
            }
            "subagent.started" => {
                if let Some(d) = data {
                    let call_id = str_field(d, "toolCallId");
                    let name = str_field(d, "agentDisplayName");
                    if !call_id.is_empty() {
                        active_agents.insert(call_id, name);
                    }
                }
            }
            "subagent.completed" => {
                if let Some(d) = data {
                    let call_id = str_field(d, "toolCallId");
                    active_agents.remove(&call_id);
                }
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

    Ok(Session {
        id: session_id,
        source: SessionSource::CopilotCli,
        source_path: path.to_path_buf(),
        title,
        model,
        started_at,
        messages,
        token_usage,
        metadata,
    })
}

struct ToolStartInfo {
    tool_name: String,
}

fn str_field(val: &serde_json::Value, key: &str) -> String {
    val.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn extract_timestamp(record: &serde_json::Value) -> Option<String> {
    record
        .get("timestamp")
        .and_then(|t| t.as_str())
        .map(String::from)
}

fn parse_session_start(
    data: Option<&serde_json::Value>,
    session_id: &mut String,
    started_at: &mut Option<String>,
    metadata: &mut SessionMetadata,
) {
    let Some(d) = data else { return };
    *session_id = str_field(d, "sessionId");
    *started_at = d.get("startTime").and_then(|t| t.as_str()).map(String::from);

    if let Some(ctx) = d.get("context") {
        metadata.working_directory = ctx.get("cwd").and_then(|v| v.as_str()).map(String::from);
        metadata.git_branch = ctx.get("branch").and_then(|v| v.as_str()).map(String::from);
        metadata.repository = ctx
            .get("repository")
            .and_then(|v| v.as_str())
            .map(String::from);
    }
}

fn parse_mode_change(data: Option<&serde_json::Value>) -> MessageMode {
    let Some(d) = data else {
        return MessageMode::Normal;
    };
    match d.get("newMode").and_then(|m| m.as_str()) {
        Some("plan") => MessageMode::Plan,
        Some("autopilot") => MessageMode::Auto,
        _ => MessageMode::Normal,
    }
}

fn parse_shutdown_usage(data: Option<&serde_json::Value>) -> Option<TokenUsage> {
    let d = data?;
    let metrics = d.get("modelMetrics")?.as_object()?;

    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;
    let mut cache_read: u64 = 0;
    let mut cache_write: u64 = 0;

    for (_model, m) in metrics {
        if let Some(usage) = m.get("usage") {
            input_tokens += usage.get("inputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
            output_tokens += usage.get("outputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
            cache_read += usage
                .get("cacheReadTokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            cache_write += usage
                .get("cacheWriteTokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
        }
    }

    Some(TokenUsage {
        input_tokens,
        output_tokens,
        cache_read_tokens: if cache_read > 0 { Some(cache_read) } else { None },
        cache_write_tokens: if cache_write > 0 {
            Some(cache_write)
        } else {
            None
        },
    })
}

fn extract_current_model(data: Option<&serde_json::Value>) -> Option<String> {
    data?.get("currentModel")?.as_str().map(String::from)
}

fn extract_message_model(_data: Option<&serde_json::Value>) -> Option<String> {
    None
}

fn parse_session_event(
    data: Option<&serde_json::Value>,
    timestamp: Option<String>,
    mode: &MessageMode,
) -> Message {
    let text = data
        .and_then(|d| d.get("message").and_then(|m| m.as_str()))
        .unwrap_or("")
        .to_string();

    Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: Role::System,
        timestamp,
        content: vec![ContentBlock::Text { text }],
        mode: mode.clone(),
        is_agent: false,
        is_meta: false,
    }
}

fn parse_user_message(
    data: Option<&serde_json::Value>,
    timestamp: Option<String>,
    mode: &MessageMode,
) -> Message {
    let text = data
        .and_then(|d| d.get("content").and_then(|c| c.as_str()))
        .unwrap_or("")
        .to_string();

    Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: Role::User,
        timestamp,
        content: vec![ContentBlock::Text { text }],
        mode: mode.clone(),
        is_agent: false,
        is_meta: false,
    }
}

fn parse_assistant_message(
    data: Option<&serde_json::Value>,
    timestamp: Option<String>,
    mode: &MessageMode,
    active_agents: &HashMap<String, String>,
) -> (Message, Vec<(String, String, serde_json::Value)>) {
    let Some(d) = data else {
        return (
            Message {
                id: uuid::Uuid::new_v4().to_string(),
                role: Role::Assistant,
                timestamp,
                content: Vec::new(),
                mode: mode.clone(),
                is_agent: false,
                is_meta: false,
            },
            Vec::new(),
        );
    };

    let mut content = Vec::new();
    let mut tool_requests = Vec::new();

    let text = d
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("");
    if !text.is_empty() {
        content.push(ContentBlock::Text {
            text: text.to_string(),
        });
    }

    if let Some(reasoning) = d.get("reasoningText").and_then(|r| r.as_str()) {
        if !reasoning.is_empty() {
            content.push(ContentBlock::Thinking {
                text: reasoning.to_string(),
            });
        }
    }

    if let Some(requests) = d.get("toolRequests").and_then(|t| t.as_array()) {
        for req in requests {
            let call_id = str_field(req, "toolCallId");
            let tool_name = str_field(req, "name");
            let input = parse_tool_arguments(req);

            content.push(ContentBlock::ToolUse {
                tool_name: tool_name.clone(),
                input: input.clone(),
                output: None,
                duration_ms: None,
            });

            tool_requests.push((call_id, tool_name, input));
        }
    }

    let is_agent = d
        .get("parentToolCallId")
        .and_then(|id| id.as_str())
        .map(|id| active_agents.contains_key(id))
        .unwrap_or(false);

    let msg = Message {
        id: d
            .get("messageId")
            .and_then(|m| m.as_str())
            .map(String::from)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        role: Role::Assistant,
        timestamp,
        content,
        mode: mode.clone(),
        is_agent,
        is_meta: false,
    };

    (msg, tool_requests)
}

fn parse_tool_arguments(req: &serde_json::Value) -> serde_json::Value {
    let args = req.get("arguments").cloned().unwrap_or(serde_json::Value::Null);
    if let Some(s) = args.as_str() {
        serde_json::from_str(s).unwrap_or(serde_json::Value::String(s.to_string()))
    } else {
        args
    }
}

fn attach_tool_completion(
    data: Option<&serde_json::Value>,
    tool_starts: &HashMap<String, ToolStartInfo>,
    messages: &mut [Message],
) {
    let Some(d) = data else { return };
    let call_id = str_field(d, "toolCallId");
    if call_id.is_empty() {
        return;
    }

    let output = d
        .get("result")
        .and_then(|r| {
            r.get("detailedContent")
                .or_else(|| r.get("content"))
                .and_then(|c| c.as_str())
        })
        .or_else(|| {
            d.get("error")
                .and_then(|e| e.get("message").and_then(|m| m.as_str()))
        })
        .map(String::from);

    let Some(start_info) = tool_starts.get(&call_id) else {
        return;
    };

    for msg in messages.iter_mut().rev() {
        for block in msg.content.iter_mut() {
            if let ContentBlock::ToolUse {
                tool_name,
                output: ref mut out,
                ..
            } = block
            {
                if *tool_name == start_info.tool_name && out.is_none() {
                    *out = output;
                    return;
                }
            }
        }
    }
}

fn title_from_first_user_message(messages: &[Message]) -> Option<String> {
    messages
        .iter()
        .find(|m| m.role == Role::User)
        .and_then(|m| m.content.first())
        .and_then(|c| match c {
            ContentBlock::Text { text } => Some(text.chars().take(100).collect()),
            _ => None,
        })
}

pub fn scan_copilot_summary(
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
            "session.start" => {
                if let Some(d) = record.get("data") {
                    started_at = d.get("startTime").and_then(|t| t.as_str()).map(String::from);
                }
            }
            "session.shutdown" => {
                if let Some(d) = record.get("data") {
                    if model.is_none() {
                        model = d.get("currentModel").and_then(|m| m.as_str()).map(String::from);
                    }
                }
            }
            "user.message" => {
                message_count += 1;
                if title.is_none() {
                    title = record
                        .get("data")
                        .and_then(|d| d.get("content"))
                        .and_then(|c| c.as_str())
                        .map(|s| s.chars().take(100).collect());
                }
            }
            "assistant.message" => {
                message_count += 1;
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
    fn test_parse_basic_copilot_session() {
        let jsonl = r#"{"type":"session.start","data":{"sessionId":"s1","version":1,"producer":"copilot-agent","copilotVersion":"1.0","startTime":"2026-03-06T22:33:43.047Z","context":{"cwd":"/home/user/project","gitRoot":"/home/user/project","branch":"main","repository":"user/project"}},"id":"e1","timestamp":"2026-03-06T22:33:43.106Z","parentId":null}
{"type":"user.message","data":{"content":"Fix the bug in auth","agentMode":"interactive"},"id":"e2","timestamp":"2026-03-06T22:34:00.000Z","parentId":"e1"}
{"type":"assistant.message","data":{"messageId":"m1","content":"I'll look into the auth module.","outputTokens":20,"toolRequests":[]},"id":"e3","timestamp":"2026-03-06T22:34:05.000Z","parentId":"e2"}"#;

        let file = write_jsonl(jsonl);
        let session = parse_copilot_session(file.path()).unwrap();

        assert_eq!(session.id, "s1");
        assert_eq!(session.source, SessionSource::CopilotCli);
        assert_eq!(session.title, Some("Fix the bug in auth".to_string()));
        assert_eq!(session.metadata.git_branch, Some("main".to_string()));
        assert_eq!(session.metadata.repository, Some("user/project".to_string()));
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, Role::User);
        assert_eq!(session.messages[1].role, Role::Assistant);
    }

    #[test]
    fn test_mode_changes_propagate() {
        let jsonl = r#"{"type":"session.start","data":{"sessionId":"s1","version":1,"producer":"copilot-agent","copilotVersion":"1.0","startTime":"2026-01-01T00:00:00Z","context":{"cwd":"/tmp","gitRoot":"/tmp","branch":"main","repository":"test"}},"id":"e1","timestamp":"2026-01-01T00:00:00Z","parentId":null}
{"type":"user.message","data":{"content":"Plan this","agentMode":"interactive"},"id":"e2","timestamp":"2026-01-01T00:00:01Z","parentId":"e1"}
{"type":"session.mode_changed","data":{"previousMode":"interactive","newMode":"plan"},"id":"e3","timestamp":"2026-01-01T00:00:02Z","parentId":"e2"}
{"type":"assistant.message","data":{"messageId":"m1","content":"Here is the plan.","outputTokens":10,"toolRequests":[]},"id":"e4","timestamp":"2026-01-01T00:00:03Z","parentId":"e3"}
{"type":"session.mode_changed","data":{"previousMode":"plan","newMode":"autopilot"},"id":"e5","timestamp":"2026-01-01T00:00:04Z","parentId":"e4"}
{"type":"assistant.message","data":{"messageId":"m2","content":"Executing...","outputTokens":10,"toolRequests":[]},"id":"e6","timestamp":"2026-01-01T00:00:05Z","parentId":"e5"}"#;

        let file = write_jsonl(jsonl);
        let session = parse_copilot_session(file.path()).unwrap();

        assert_eq!(session.messages[0].mode, MessageMode::Normal);
        assert_eq!(session.messages[1].mode, MessageMode::Plan);
        assert_eq!(session.messages[2].mode, MessageMode::Auto);
    }

    #[test]
    fn test_tool_results_attached() {
        let jsonl = r#"{"type":"session.start","data":{"sessionId":"s1","version":1,"producer":"copilot-agent","copilotVersion":"1.0","startTime":"2026-01-01T00:00:00Z","context":{"cwd":"/tmp","gitRoot":"/tmp","branch":"main","repository":"test"}},"id":"e1","timestamp":"2026-01-01T00:00:00Z","parentId":null}
{"type":"assistant.message","data":{"messageId":"m1","content":"","outputTokens":10,"toolRequests":[{"toolCallId":"call_1","name":"read_file","arguments":"{\"path\":\"test.txt\"}","type":"function"}]},"id":"e2","timestamp":"2026-01-01T00:00:01Z","parentId":"e1"}
{"type":"tool.execution_complete","data":{"toolCallId":"call_1","success":true,"result":{"content":"file data","detailedContent":"full file data"}},"id":"e3","timestamp":"2026-01-01T00:00:02Z","parentId":"e2"}"#;

        let file = write_jsonl(jsonl);
        let session = parse_copilot_session(file.path()).unwrap();

        assert_eq!(session.messages.len(), 1);
        if let ContentBlock::ToolUse { tool_name, output, .. } = &session.messages[0].content[0] {
            assert_eq!(tool_name, "read_file");
            assert_eq!(output, &Some("full file data".to_string()));
        } else {
            panic!("Expected ToolUse block");
        }
    }

    #[test]
    fn test_shutdown_extracts_usage() {
        let jsonl = r#"{"type":"session.start","data":{"sessionId":"s1","version":1,"producer":"copilot-agent","copilotVersion":"1.0","startTime":"2026-01-01T00:00:00Z","context":{"cwd":"/tmp","gitRoot":"/tmp","branch":"main","repository":"test"}},"id":"e1","timestamp":"2026-01-01T00:00:00Z","parentId":null}
{"type":"session.shutdown","data":{"shutdownType":"normal","totalPremiumRequests":5,"totalApiDurationMs":1000,"sessionStartTime":0,"codeChanges":{"linesAdded":10,"linesRemoved":2,"filesModified":[]},"modelMetrics":{"gpt-5":{"requests":{"count":5,"cost":0},"usage":{"inputTokens":5000,"outputTokens":1000,"cacheReadTokens":200,"cacheWriteTokens":100}}},"currentModel":"gpt-5"},"id":"e2","timestamp":"2026-01-01T00:01:00Z","parentId":"e1"}"#;

        let file = write_jsonl(jsonl);
        let session = parse_copilot_session(file.path()).unwrap();

        assert_eq!(session.model, Some("gpt-5".to_string()));
        let usage = session.token_usage.unwrap();
        assert_eq!(usage.input_tokens, 5000);
        assert_eq!(usage.output_tokens, 1000);
        assert_eq!(usage.cache_read_tokens, Some(200));
    }
}
