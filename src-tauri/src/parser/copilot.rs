use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::records::{
    CopilotAssistantData, CopilotRecord, CopilotShutdownData, CopilotToolCompleteData,
    CopilotUserMessageData,
};
use super::{truncate_to_chars, ParseEvent, ParseState, UsageAccumulator};
use crate::error::AppError;
use crate::model::{
    ContentBlock, Message, MessageMode, Role, Session, SessionMetadata, SessionSource, TokenUsage,
};

pub fn parse_copilot_session(path: &Path) -> Result<Session, AppError> {
    let mut active_agents: HashMap<String, String> = HashMap::new();

    let session_id = super::session_id_from_path(path);
    let mut session = super::parse_jsonl_session(
        path,
        SessionSource::CopilotCli,
        session_id,
        |line, state| {
            let Ok(record) = serde_json::from_str::<CopilotRecord>(line) else { return };
            process_copilot_record(&record, state, &mut active_agents);
        },
    )?;

    if session.id.is_empty() {
        session.id = super::session_id_from_path(path);
    }

    Ok(session)
}

pub fn parse_copilot_content(filename: &str, content: &str) -> Result<Session, AppError> {
    let mut active_agents: HashMap<String, String> = HashMap::new();
    let path = std::path::Path::new(filename);
    let session_id = super::session_id_from_path(path);

    super::parse_jsonl_from_content(
        content,
        SessionSource::CopilotCli,
        session_id,
        path,
        |line, state| {
            let Ok(record) = serde_json::from_str::<CopilotRecord>(line) else { return };
            process_copilot_record(&record, state, &mut active_agents);
        },
    )
}

fn process_copilot_record(
    record: &CopilotRecord,
    state: &mut ParseState,
    active_agents: &mut HashMap<String, String>,
) {
    let timestamp = record.timestamp().map(String::from);

    match record {
        CopilotRecord::SessionStart { data: Some(d), .. } => {
            if let Some(sid) = &d.session_id {
                state.apply(ParseEvent::SetSessionId(sid.clone()));
            }
            if let Some(ts) = &d.start_time {
                state.apply(ParseEvent::SetStartedAt(ts.clone()));
            }
            if let Some(ctx) = &d.context {
                state.apply(ParseEvent::SetMetadata(SessionMetadata {
                    working_directory: ctx.cwd.clone(),
                    git_branch: ctx.branch.clone(),
                    repository: ctx.repository.clone(),
                    ..SessionMetadata::default()
                }));
            }
        }

        CopilotRecord::ModeChanged { data, .. } => {
            let mode = data
                .as_ref()
                .and_then(|d| d.new_mode.as_deref())
                .map(super::parse_mode)
                .unwrap_or(MessageMode::Normal);
            state.apply(ParseEvent::SetMode(mode));
        }

        CopilotRecord::SessionShutdown { data: Some(d), .. } => {
            if let Some(tu) = parse_shutdown_usage(d) {
                state.apply(ParseEvent::SetTokenUsage(Some(tu)));
            }
            if let Some(m) = &d.current_model {
                state.apply(ParseEvent::SetModel(m.clone()));
            }
        }

        CopilotRecord::SessionWarning { data, .. }
        | CopilotRecord::SessionError { data, .. } => {
            let text = data
                .as_ref()
                .and_then(|d| d.message.clone())
                .unwrap_or_default();
            state.apply(ParseEvent::AddMessage(Message {
                id: uuid::Uuid::new_v4().to_string(),
                role: Role::System,
                timestamp,
                content: vec![ContentBlock::Text { text }],
                mode: state.current_mode,
                is_agent: false,
                is_meta: false,
                duration_ms: None,
            }));
        }

        CopilotRecord::UserMessage { data, .. } => {
            state.apply(ParseEvent::AddMessage(
                build_user_message(data.as_ref(), timestamp, state.current_mode),
            ));
        }

        CopilotRecord::AssistantMessage { data, .. } => {
            let msg = build_assistant_message(data.as_ref(), timestamp, state.current_mode, active_agents);
            state.apply(ParseEvent::AddMessage(msg));
        }

        CopilotRecord::ToolComplete { data: Some(d), .. } => {
            match d.tool_call_id.as_deref() {
                Some(id) if !id.is_empty() => {
                    state.apply(ParseEvent::AttachToolOutputById {
                        tool_call_id: id.to_string(),
                        output: extract_tool_completion_output(d),
                        duration_ms: None,
                    });
                }
                _ => {}
            }
        }

        CopilotRecord::SubagentStarted { data: Some(d), .. } => {
            if let Some(call_id) = d.tool_call_id.as_deref().filter(|id| !id.is_empty()) {
                let name = d.agent_display_name.clone().unwrap_or_default();
                active_agents.insert(call_id.to_owned(), name);
            }
        }

        CopilotRecord::SubagentCompleted { data: Some(d), .. } => {
            if let Some(call_id) = &d.tool_call_id {
                active_agents.remove(call_id);
            }
        }

        CopilotRecord::SessionStart { data: None, .. }
        | CopilotRecord::SessionShutdown { data: None, .. }
        | CopilotRecord::ToolComplete { data: None, .. }
        | CopilotRecord::SubagentStarted { data: None, .. }
        | CopilotRecord::SubagentCompleted { data: None, .. }
        | CopilotRecord::Unknown => {}
    }
}

fn parse_shutdown_usage(data: &CopilotShutdownData) -> Option<TokenUsage> {
    let metrics = data.model_metrics.as_ref()?;
    let acc = metrics
        .values()
        .filter_map(|m| m.get("usage"))
        .fold(UsageAccumulator::default(), |acc, usage| acc.merge(usage));
    acc.into_token_usage()
}

fn extract_tool_completion_output(d: &CopilotToolCompleteData) -> Option<String> {
    d.result
        .as_ref()
        .and_then(|r| r.detailed_content.as_deref().or(r.content.as_deref()))
        .or_else(|| d.error.as_ref().and_then(|e| e.message.as_deref()))
        .map(String::from)
}

fn build_user_message(
    data: Option<&CopilotUserMessageData>,
    timestamp: Option<String>,
    mode: MessageMode,
) -> Message {
    let mut content: Vec<ContentBlock> = match data {
        Some(d) => {
            let mut blocks: Vec<ContentBlock> = d
                .content
                .as_deref()
                .filter(|t| !t.is_empty())
                .into_iter()
                .map(|text| ContentBlock::Text { text: text.to_owned() })
                .collect();

            if let Some(attachments) = &d.attachments {
                blocks.extend(attachments.iter().filter_map(attachment_to_image));
            }

            blocks
        }
        None => Vec::new(),
    };

    if content.is_empty() {
        content.push(ContentBlock::Text {
            text: String::new(),
        });
    }

    Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: Role::User,
        timestamp,
        content,
        mode,
        is_agent: false,
        is_meta: false,
        duration_ms: None,
    }
}

fn attachment_to_image(
    att: &super::records::CopilotAttachment,
) -> Option<ContentBlock> {
    if att.kind.as_deref() != Some("image") {
        return None;
    }
    match (&att.data, &att.url) {
        (Some(data_str), _) => {
            let media_type = att.media_type.as_deref().unwrap_or("image/png");
            Some(ContentBlock::Image {
                source: format!("data:{media_type};base64,{data_str}"),
            })
        }
        (None, Some(url)) => Some(ContentBlock::Image {
            source: url.clone(),
        }),
        (None, None) => None,
    }
}

fn build_assistant_message(
    data: Option<&CopilotAssistantData>,
    timestamp: Option<String>,
    mode: MessageMode,
    active_agents: &HashMap<String, String>,
) -> Message {
    let Some(d) = data else {
        return Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: Role::Assistant,
            timestamp,
            content: Vec::new(),
            mode,
            is_agent: false,
            is_meta: false,
            duration_ms: None,
        };
    };

    let mut content: Vec<ContentBlock> = Vec::new();

    if let Some(text) = d.content.as_deref().filter(|t| !t.is_empty()) {
        content.push(ContentBlock::Text {
            text: text.to_owned(),
        });
    }

    if let Some(reasoning) = d.reasoning_text.as_deref().filter(|r| !r.is_empty()) {
        content.push(ContentBlock::Thinking {
            text: reasoning.to_owned(),
        });
    }

    content.extend(
        d.tool_requests
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|req| ContentBlock::ToolUse {
                tool_name: req.name.clone().unwrap_or_default(),
                tool_call_id: req.tool_call_id.clone(),
                input: req.arguments.as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or(serde_json::Value::Null),
                output: None,
                duration_ms: None,
            }),
    );

    let is_agent = d
        .parent_tool_call_id
        .as_deref()
        .is_some_and(|id| active_agents.contains_key(id));

    Message {
        id: d
            .message_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        role: Role::Assistant,
        timestamp,
        content,
        mode,
        is_agent,
        is_meta: false,
        duration_ms: None,
    }
}

/// Quickly scans a Copilot CLI session file for summary metadata without full parsing.
pub fn scan_copilot_summary(path: &Path) -> super::ScanSummary {
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

        match record.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "session.start" => {
                started_at = record
                    .get("data")
                    .and_then(|d| d.get("startTime"))
                    .and_then(|t| t.as_str())
                    .map(String::from);
            }
            "session.shutdown" => {
                model = model.or_else(|| {
                    record.get("data")?.get("currentModel")?.as_str().map(String::from)
                });
            }
            "user.message" => {
                message_count += 1;
                title = title.or_else(|| {
                    record
                        .get("data")?
                        .get("content")?
                        .as_str()
                        .map(|s| truncate_to_chars(s, 100))
                });
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
