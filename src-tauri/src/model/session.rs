use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The origin platform of a parsed session.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionSource {
    ClaudeCode,
    CopilotCli,
    Codex,
}

/// The permission/interaction mode active during a message.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MessageMode {
    #[default]
    Normal,
    Plan,
    Auto,
}

/// A complete parsed conversation session with messages and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub source: SessionSource,
    pub source_path: PathBuf,
    pub title: Option<String>,
    pub model: Option<String>,
    pub started_at: Option<String>,
    pub messages: Vec<Message>,
    pub token_usage: Option<TokenUsage>,
    pub metadata: SessionMetadata,
}

/// A single message within a session conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub role: Role,
    pub timestamp: Option<String>,
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub mode: MessageMode,
    #[serde(default)]
    pub is_agent: bool,
    #[serde(default)]
    pub is_meta: bool,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

/// The role of a message sender.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

/// A typed content block within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(rename = "toolCallId", default, skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
        input: serde_json::Value,
        output: Option<String>,
        #[serde(rename = "durationMs")]
        duration_ms: Option<u64>,
    },
    Thinking {
        text: String,
    },
    CodeBlock {
        language: Option<String>,
        code: String,
    },
    Image {
        source: String,
    },
}

/// Aggregated token usage statistics for a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}

/// Project metadata extracted from session context.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadata {
    pub working_directory: Option<String>,
    pub git_branch: Option<String>,
    pub slug: Option<String>,
    pub repository: Option<String>,
}

/// Lightweight session info for sidebar listing without full message parsing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub source: SessionSource,
    pub path: PathBuf,
    pub title: Option<String>,
    pub started_at: Option<String>,
    pub message_count: usize,
    pub model: Option<String>,
}
