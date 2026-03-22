pub mod claude;
pub mod codex;
pub mod copilot;
pub mod detect;
pub mod records;

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use std::path::PathBuf;

use crate::error::AppError;
use crate::model::{ContentBlock, Message, MessageMode, Role, Session, SessionMetadata, SessionSource, SessionSummary, TokenUsage};

/// Result type for scan functions: (title, model, started_at, message_count).
pub type ScanSummary = Result<(Option<String>, Option<String>, Option<String>, usize), AppError>;

/// Unified interface for parsing session files from any supported source.
pub trait SessionParser {
    fn source(&self) -> SessionSource;
    fn parse(&self, path: &Path) -> Result<Session, AppError>;
    fn parse_content(&self, filename: &str, content: &str) -> Result<Session, AppError>;
    fn scan(&self, path: &Path) -> ScanSummary;
    fn session_id(&self, path: &Path) -> String { session_id_from_path(path) }
    fn home_subpath(&self) -> &[&str];
}

struct ClaudeParser;
struct CopilotParser;
struct CodexParser;

impl SessionParser for ClaudeParser {
    fn source(&self) -> SessionSource { SessionSource::ClaudeCode }
    fn parse(&self, path: &Path) -> Result<Session, AppError> { claude::parse_claude_session(path) }
    fn parse_content(&self, filename: &str, content: &str) -> Result<Session, AppError> { claude::parse_claude_content(filename, content) }
    fn scan(&self, path: &Path) -> ScanSummary { claude::scan_claude_summary(path) }
    fn home_subpath(&self) -> &[&str] { &[".claude", "projects"] }
}

impl SessionParser for CopilotParser {
    fn source(&self) -> SessionSource { SessionSource::CopilotCli }
    fn parse(&self, path: &Path) -> Result<Session, AppError> { copilot::parse_copilot_session(path) }
    fn parse_content(&self, filename: &str, content: &str) -> Result<Session, AppError> { copilot::parse_copilot_content(filename, content) }
    fn scan(&self, path: &Path) -> ScanSummary { copilot::scan_copilot_summary(path) }
    fn session_id(&self, path: &Path) -> String {
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    }
    fn home_subpath(&self) -> &[&str] { &[".copilot", "session-state"] }
}

impl SessionParser for CodexParser {
    fn source(&self) -> SessionSource { SessionSource::Codex }
    fn parse(&self, path: &Path) -> Result<Session, AppError> { codex::parse_codex_session(path) }
    fn parse_content(&self, filename: &str, content: &str) -> Result<Session, AppError> { codex::parse_codex_content(filename, content) }
    fn scan(&self, path: &Path) -> ScanSummary { codex::scan_codex_summary(path) }
    fn home_subpath(&self) -> &[&str] { &[".codex", "sessions"] }
}

/// All registered session parsers.
pub fn parsers() -> &'static [&'static dyn SessionParser] {
    &[&ClaudeParser, &CopilotParser, &CodexParser]
}

/// Returns the parser for a detected session source.
pub fn parser_for(source: SessionSource) -> &'static dyn SessionParser {
    parsers().iter()
        .copied()
        .find(|p| p.source() == source)
        .expect("all SessionSource variants have a parser")
}

/// Builds a SessionSummary from a file path using the given parser.
pub fn build_summary(parser: &dyn SessionParser, path: PathBuf) -> Result<SessionSummary, AppError> {
    let id = parser.session_id(&path);
    let (title, model, started_at, message_count) = parser.scan(&path)?;
    Ok(SessionSummary { id, source: parser.source(), path, title, started_at, message_count, model })
}

/// Extracts a session ID from a file path using the file stem.
pub(crate) fn session_id_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Truncates a string to at most `max` characters.
pub(crate) fn truncate_to_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Cleans up CLI command XML tags into readable text.
///
/// Converts `<command-name>/clear</command-name>...` into `/clear`.
pub(crate) fn clean_command_xml(text: &str) -> Option<String> {
    let text = text.trim();
    let name = text.strip_prefix("<command-name>")?
        .split("</command-name>")
        .next()?;
    Some(name.to_string())
}

/// Cleans raw text and returns a title if it's meaningful.
///
/// Applies command XML cleanup, noise filtering, and truncation.
/// Used by both the full parse path and the scan/summary path.
pub(crate) fn extract_title_from_text(raw: &str) -> Option<String> {
    let cleaned = clean_command_xml(raw).unwrap_or_else(|| raw.to_string());
    let trimmed = cleaned.trim();
    if trimmed.is_empty() || trimmed.len() < 5 {
        return None;
    }
    let noise_prefixes = [
        "[Request interrupted",
        "<local-command",
        "<system-reminder",
        "<caveat",
        "Set model to",
        "<command-name",
    ];
    if noise_prefixes.iter().any(|p| trimmed.starts_with(p)) {
        return None;
    }
    Some(truncate_to_chars(trimmed, 100))
}

/// Derives a session title from the first meaningful user message.
pub(crate) fn title_from_first_user_message(messages: &[Message]) -> Option<String> {
    messages
        .iter()
        .filter(|m| m.role == Role::User && !m.is_meta)
        .find_map(|m| {
            m.content.iter().find_map(|c| match c {
                ContentBlock::Text { text } => extract_title_from_text(text),
                _ => None,
            })
        })
}

/// Parses tool call arguments from a JSON payload, handling string-encoded JSON.
pub(crate) fn parse_tool_arguments(payload: &serde_json::Value) -> serde_json::Value {
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

/// Converts a mode string to the corresponding MessageMode enum value.
pub(crate) fn parse_mode(mode_str: &str) -> MessageMode {
    match mode_str {
        "plan" => MessageMode::Plan,
        "auto" | "autopilot" | "full-auto" => MessageMode::Auto,
        _ => MessageMode::Normal,
    }
}

fn nonzero(val: u64) -> Option<u64> {
    (val > 0).then_some(val)
}

fn u64_field_any(val: &serde_json::Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|k| val.get(*k).and_then(|v| v.as_u64()))
        .unwrap_or(0)
}

#[derive(Default)]
/// Accumulates token usage across multiple records via pure fold semantics.
pub(crate) struct UsageAccumulator {
    input_tokens: u64,
    output_tokens: u64,
    cache_read: u64,
    cache_write: u64,
    has_data: bool,
}

impl UsageAccumulator {
    pub(crate) fn merge(self, usage: &serde_json::Value) -> Self {
        Self {
            input_tokens: self.input_tokens + u64_field_any(usage, &["input_tokens", "inputTokens"]),
            output_tokens: self.output_tokens + u64_field_any(usage, &["output_tokens", "outputTokens"]),
            cache_read: self.cache_read + u64_field_any(usage, &["cache_read_input_tokens", "cached_input_tokens", "cacheReadTokens"]),
            cache_write: self.cache_write + u64_field_any(usage, &["cache_creation_input_tokens", "cacheWriteTokens"]),
            has_data: true,
        }
    }

    pub(crate) fn merge_typed(self, input: u64, output: u64, cache_read: u64, cache_write: u64) -> Self {
        Self {
            input_tokens: self.input_tokens + input,
            output_tokens: self.output_tokens + output,
            cache_read: self.cache_read + cache_read,
            cache_write: self.cache_write + cache_write,
            has_data: true,
        }
    }

    pub(crate) fn into_token_usage(self) -> Option<TokenUsage> {
        self.has_data.then(|| TokenUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cache_read_tokens: nonzero(self.cache_read),
            cache_write_tokens: nonzero(self.cache_write),
        })
    }
}

pub(crate) enum ParseEvent {
    AddMessage(Message),
    SetTitle(String),
    SetModel(String),
    SetStartedAt(String),
    SetMode(MessageMode),
    SetSessionId(String),
    MergeUsage { input: u64, output: u64, cache_read: u64, cache_write: u64 },
    SetTokenUsage(Option<TokenUsage>),
    SetMetadata(SessionMetadata),
    AttachToolOutputById { tool_call_id: String, output: Option<String>, duration_ms: Option<u64> },
    AttachToolOutputBySuffix { suffix: String, output: Option<String>, duration_ms: Option<u64> },
    PushTurnItem(ContentBlock),
    SetTurnTimestamp(Option<String>),
    FlushTurn { timestamp: Option<String> },
}

pub(crate) struct ParseState {
    pub session_id: String,
    pub messages: Vec<Message>,
    pub title: Option<String>,
    pub model: Option<String>,
    pub started_at: Option<String>,
    pub current_mode: MessageMode,
    pub usage: UsageAccumulator,
    pub token_usage_override: Option<Option<TokenUsage>>,
    pub metadata: SessionMetadata,
    pub turn_items: Vec<ContentBlock>,
    pub turn_timestamp: Option<String>,
    pub tool_call_timestamps: std::collections::HashMap<String, String>,
    last_user_ts: Option<chrono::DateTime<chrono::Utc>>,
}

impl ParseState {
    fn new(session_id: String) -> Self {
        Self {
            session_id,
            messages: Vec::new(),
            title: None,
            model: None,
            started_at: None,
            current_mode: MessageMode::Normal,
            usage: UsageAccumulator::default(),
            token_usage_override: None,
            metadata: SessionMetadata::default(),
            turn_items: Vec::new(),
            turn_timestamp: None,
            tool_call_timestamps: std::collections::HashMap::new(),
            last_user_ts: None,
        }
    }

    pub(crate) fn parse_ts(iso: Option<&str>) -> Option<chrono::DateTime<chrono::Utc>> {
        iso?.parse().ok()
    }

    pub(crate) fn ms_between(
        from: Option<chrono::DateTime<chrono::Utc>>,
        to: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Option<u64> {
        let ms = to?.signed_duration_since(from?).num_milliseconds();
        (ms > 0).then_some(ms as u64)
    }

    fn push_message(&mut self, mut msg: Message) {
        let ts = Self::parse_ts(msg.timestamp.as_deref());

        if msg.role == Role::User {
            self.last_user_ts = ts.or(self.last_user_ts);
        }

        if msg.role == Role::Assistant && msg.duration_ms.is_none() {
            msg.duration_ms = Self::ms_between(self.last_user_ts, ts);
        }

        self.messages.push(msg);
    }

    pub fn apply(&mut self, event: ParseEvent) {
        match event {
            ParseEvent::AddMessage(msg) => self.push_message(msg),
            ParseEvent::SetTitle(t) => { self.title.get_or_insert(t); }
            ParseEvent::SetModel(m) => { self.model.get_or_insert(m); }
            ParseEvent::SetStartedAt(ts) => { self.started_at.get_or_insert(ts); }
            ParseEvent::SetMode(mode) => self.current_mode = mode,
            ParseEvent::SetSessionId(id) => self.session_id = id,
            ParseEvent::MergeUsage { input, output, cache_read, cache_write } => {
                self.usage = std::mem::take(&mut self.usage)
                    .merge_typed(input, output, cache_read, cache_write);
            }
            ParseEvent::SetTokenUsage(tu) => {
                self.token_usage_override = Some(tu);
            }
            ParseEvent::SetMetadata(meta) => self.metadata = meta,
            ParseEvent::AttachToolOutputById { tool_call_id, output, duration_ms } => {
                attach_tool_output_by_id(&mut self.messages, &tool_call_id, output, duration_ms);
            }
            ParseEvent::AttachToolOutputBySuffix { suffix, output, duration_ms } => {
                attach_tool_output_by_suffix(&mut self.turn_items, &mut self.messages, &suffix, output, duration_ms);
            }
            ParseEvent::PushTurnItem(block) => self.turn_items.push(block),
            ParseEvent::SetTurnTimestamp(ts) => self.turn_timestamp = ts,
            ParseEvent::FlushTurn { timestamp } => {
                if !self.turn_items.is_empty() {
                    let start = Self::parse_ts(self.turn_timestamp.as_deref())
                        .or(self.last_user_ts);
                    let end_ts = timestamp.or(self.turn_timestamp.take());
                    let end = Self::parse_ts(end_ts.as_deref());
                    let content = std::mem::take(&mut self.turn_items);
                    self.push_message(Message {
                        id: uuid::Uuid::new_v4().to_string(),
                        role: Role::Assistant,
                        timestamp: end_ts,
                        content,
                        mode: self.current_mode,
                        is_agent: false,
                        is_meta: false,
                        duration_ms: Self::ms_between(start, end),
                    });
                }
                self.turn_timestamp = None;
            }
        }
    }

    fn into_session(mut self, path: &Path, source: SessionSource) -> Session {
        if !self.turn_items.is_empty() {
            self.apply(ParseEvent::FlushTurn { timestamp: None });
        }

        if self.title.is_none() {
            self.title = title_from_first_user_message(&self.messages);
        }

        let token_usage = match self.token_usage_override {
            Some(tu) => tu,
            None => self.usage.into_token_usage(),
        };

        Session {
            id: self.session_id,
            source,
            source_path: path.to_path_buf(),
            title: self.title,
            model: self.model,
            started_at: self.started_at,
            messages: self.messages,
            token_usage,
            metadata: self.metadata,
        }
    }
}

fn attach_tool_output_by_id(messages: &mut [Message], id: &str, output: Option<String>, duration_ms: Option<u64>) {
    let block = messages.iter_mut().rev()
        .flat_map(|msg| msg.content.iter_mut().rev())
        .find(|b| matches!(b, ContentBlock::ToolUse { tool_call_id: Some(ref cid), output: None, .. } if cid == id));
    if let Some(ContentBlock::ToolUse { output: out, duration_ms: dur, .. }) = block {
        *out = output;
        *dur = duration_ms;
    }
}

fn find_tool_by_suffix<'a>(blocks: &'a mut [ContentBlock], suffix: &str) -> Option<&'a mut ContentBlock> {
    blocks.iter_mut().rev()
        .find(|b| matches!(b, ContentBlock::ToolUse { tool_name, output: None, .. } if tool_name.ends_with(suffix)))
}

fn attach_tool_output_by_suffix(
    turn_items: &mut [ContentBlock],
    messages: &mut [Message],
    suffix: &str,
    output: Option<String>,
    duration_ms: Option<u64>,
) {
    let block = find_tool_by_suffix(turn_items, suffix)
        .or_else(|| messages.iter_mut().rev()
            .find_map(|msg| find_tool_by_suffix(&mut msg.content, suffix)));

    if let Some(ContentBlock::ToolUse { tool_name, output: out, duration_ms: dur, .. }) = block {
        *tool_name = tool_name.strip_suffix(suffix).unwrap_or(tool_name).to_string();
        *out = output;
        *dur = duration_ms;
    }
}

pub(crate) fn parse_jsonl_session<F>(
    path: &Path,
    source: SessionSource,
    session_id: String,
    process_line: F,
) -> Result<Session, AppError>
where
    F: FnMut(&str, &mut ParseState),
{
    let file = File::open(path)?;
    let reader = BufReader::with_capacity(64 * 1024, file);
    let state = parse_lines(reader, session_id, process_line);
    Ok(state.into_session(path, source))
}

pub(crate) fn parse_jsonl_from_content<F>(
    content: &str,
    source: SessionSource,
    session_id: String,
    source_path: &Path,
    process_line: F,
) -> Result<Session, AppError>
where
    F: FnMut(&str, &mut ParseState),
{
    let reader = std::io::Cursor::new(content.as_bytes());
    let state = parse_lines(reader, session_id, process_line);
    Ok(state.into_session(source_path, source))
}

fn parse_lines<R: std::io::Read, F>(
    reader: R,
    session_id: String,
    mut process_line: F,
) -> ParseState
where
    F: FnMut(&str, &mut ParseState),
{
    let reader = BufReader::with_capacity(64 * 1024, reader);
    let mut state = ParseState::new(session_id);

    for line in reader.lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if !line.is_empty() {
            process_line(line, &mut state);
        }
    }

    state
}

