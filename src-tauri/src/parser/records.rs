use serde::Deserialize;

// Claude Code records

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeRecord {
    User {
        message: Option<ClaudeMessagePayload>,
        timestamp: Option<String>,
        #[serde(rename = "sessionId")]
        session_id: Option<String>,
        slug: Option<String>,
        #[serde(rename = "gitBranch")]
        git_branch: Option<String>,
        #[serde(rename = "permissionMode")]
        permission_mode: Option<String>,
        uuid: Option<String>,
        #[serde(rename = "isSidechain")]
        is_sidechain: Option<bool>,
        #[serde(rename = "isMeta")]
        is_meta: Option<bool>,
        #[serde(rename = "durationMs")]
        duration_ms: Option<u64>,
    },
    Assistant {
        message: Option<ClaudeAssistantMessage>,
        timestamp: Option<String>,
        #[serde(rename = "sessionId")]
        session_id: Option<String>,
        uuid: Option<String>,
        #[serde(rename = "isSidechain")]
        is_sidechain: Option<bool>,
        #[serde(rename = "isMeta")]
        is_meta: Option<bool>,
        #[serde(rename = "durationMs")]
        duration_ms: Option<u64>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: Option<String>,
        content: Option<serde_json::Value>,
        duration_ms: Option<u64>,
        #[serde(rename = "sessionId")]
        session_id: Option<String>,
    },
    Summary {
        summary: Option<String>,
        #[serde(rename = "isCompactSummary")]
        is_compact_summary: Option<bool>,
        #[serde(rename = "sessionId")]
        session_id: Option<String>,
    },
    System {
        message: Option<ClaudeMessagePayload>,
        timestamp: Option<String>,
        #[serde(rename = "sessionId")]
        session_id: Option<String>,
        cwd: Option<String>,
        #[serde(rename = "workingDirectory")]
        working_directory: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

impl ClaudeRecord {
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::User { session_id, .. }
            | Self::Assistant { session_id, .. }
            | Self::ToolResult { session_id, .. }
            | Self::Summary { session_id, .. }
            | Self::System { session_id, .. } => session_id.as_deref(),
            Self::Unknown => None,
        }
    }

}

#[derive(Deserialize)]
pub struct ClaudeMessagePayload {
    pub content: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct ClaudeAssistantMessage {
    pub model: Option<String>,
    pub content: Option<Vec<ClaudeContentBlock>>,
    pub usage: Option<ClaudeUsage>,
}

#[derive(Deserialize)]
pub struct ClaudeUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeContentBlock {
    Text { text: Option<String> },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: Option<String>,
        name: Option<String>,
        input: Option<serde_json::Value>,
    },
    Thinking { thinking: Option<String> },
    Image { source: Option<serde_json::Value> },
    #[serde(other)]
    Unknown,
}

// Copilot CLI records

#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum CopilotRecord {
    #[serde(rename = "session.start")]
    SessionStart {
        data: Option<CopilotSessionStartData>,
        timestamp: Option<String>,
    },
    #[serde(rename = "session.mode_changed")]
    ModeChanged {
        data: Option<CopilotModeChangeData>,
        timestamp: Option<String>,
    },
    #[serde(rename = "session.shutdown")]
    SessionShutdown {
        data: Option<CopilotShutdownData>,
        timestamp: Option<String>,
    },
    #[serde(rename = "session.warning")]
    SessionWarning {
        data: Option<CopilotEventData>,
        timestamp: Option<String>,
    },
    #[serde(rename = "session.error")]
    SessionError {
        data: Option<CopilotEventData>,
        timestamp: Option<String>,
    },
    #[serde(rename = "user.message")]
    UserMessage {
        data: Option<CopilotUserMessageData>,
        timestamp: Option<String>,
    },
    #[serde(rename = "assistant.message")]
    AssistantMessage {
        data: Option<CopilotAssistantData>,
        timestamp: Option<String>,
    },
    #[serde(rename = "tool.execution_complete")]
    ToolComplete {
        data: Option<CopilotToolCompleteData>,
        timestamp: Option<String>,
    },
    #[serde(rename = "subagent.started")]
    SubagentStarted {
        data: Option<CopilotSubagentData>,
        timestamp: Option<String>,
    },
    #[serde(rename = "subagent.completed")]
    SubagentCompleted {
        data: Option<CopilotSubagentData>,
        timestamp: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

impl CopilotRecord {
    pub fn timestamp(&self) -> Option<&str> {
        match self {
            Self::SessionStart { timestamp, .. }
            | Self::ModeChanged { timestamp, .. }
            | Self::SessionShutdown { timestamp, .. }
            | Self::SessionWarning { timestamp, .. }
            | Self::SessionError { timestamp, .. }
            | Self::UserMessage { timestamp, .. }
            | Self::AssistantMessage { timestamp, .. }
            | Self::ToolComplete { timestamp, .. }
            | Self::SubagentStarted { timestamp, .. }
            | Self::SubagentCompleted { timestamp, .. } => timestamp.as_deref(),
            Self::Unknown => None,
        }
    }
}

#[derive(Deserialize)]
pub struct CopilotSessionStartData {
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    #[serde(rename = "startTime")]
    pub start_time: Option<String>,
    pub context: Option<CopilotContext>,
}

#[derive(Deserialize)]
pub struct CopilotContext {
    pub cwd: Option<String>,
    pub branch: Option<String>,
    pub repository: Option<String>,
}

#[derive(Deserialize)]
pub struct CopilotModeChangeData {
    #[serde(rename = "newMode")]
    pub new_mode: Option<String>,
}

#[derive(Deserialize)]
pub struct CopilotShutdownData {
    #[serde(rename = "currentModel")]
    pub current_model: Option<String>,
    #[serde(rename = "modelMetrics")]
    pub model_metrics: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Deserialize)]
pub struct CopilotEventData {
    pub message: Option<String>,
}

#[derive(Deserialize)]
pub struct CopilotUserMessageData {
    pub content: Option<String>,
    pub attachments: Option<Vec<CopilotAttachment>>,
}

#[derive(Deserialize)]
pub struct CopilotAttachment {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    #[serde(rename = "mediaType")]
    pub media_type: Option<String>,
    pub data: Option<String>,
    pub url: Option<String>,
}

#[derive(Deserialize)]
pub struct CopilotAssistantData {
    #[serde(rename = "messageId")]
    pub message_id: Option<String>,
    pub content: Option<String>,
    #[serde(rename = "reasoningText")]
    pub reasoning_text: Option<String>,
    #[serde(rename = "toolRequests")]
    pub tool_requests: Option<Vec<CopilotToolRequest>>,
    #[serde(rename = "parentToolCallId")]
    pub parent_tool_call_id: Option<String>,
}

#[derive(Deserialize)]
pub struct CopilotToolRequest {
    #[serde(rename = "toolCallId")]
    pub tool_call_id: Option<String>,
    pub name: Option<String>,
    pub arguments: Option<String>,
}

#[derive(Deserialize)]
pub struct CopilotToolCompleteData {
    #[serde(rename = "toolCallId")]
    pub tool_call_id: Option<String>,
    pub result: Option<CopilotToolResult>,
    pub error: Option<CopilotToolError>,
}

#[derive(Deserialize)]
pub struct CopilotToolResult {
    pub content: Option<String>,
    #[serde(rename = "detailedContent")]
    pub detailed_content: Option<String>,
}

#[derive(Deserialize)]
pub struct CopilotToolError {
    pub message: Option<String>,
}

#[derive(Deserialize)]
pub struct CopilotSubagentData {
    #[serde(rename = "toolCallId")]
    pub tool_call_id: Option<String>,
    #[serde(rename = "agentDisplayName")]
    pub agent_display_name: Option<String>,
}

// Codex CLI records

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CodexRecord {
    #[serde(rename = "session_meta")]
    SessionMeta {
        payload: Option<CodexSessionMeta>,
        timestamp: Option<String>,
    },
    #[serde(rename = "turn_context")]
    TurnContext {
        payload: Option<CodexTurnContext>,
        timestamp: Option<String>,
    },
    #[serde(rename = "event_msg")]
    EventMsg {
        payload: Option<serde_json::Value>,
        timestamp: Option<String>,
    },
    #[serde(rename = "response_item")]
    ResponseItem {
        payload: Option<serde_json::Value>,
        timestamp: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

impl CodexRecord {
    pub fn timestamp(&self) -> Option<&str> {
        match self {
            Self::SessionMeta { timestamp, .. }
            | Self::TurnContext { timestamp, .. }
            | Self::EventMsg { timestamp, .. }
            | Self::ResponseItem { timestamp, .. } => timestamp.as_deref(),
            Self::Unknown => None,
        }
    }
}

#[derive(Deserialize)]
pub struct CodexSessionMeta {
    pub id: Option<String>,
    pub timestamp: Option<String>,
    pub cwd: Option<String>,
    pub git: Option<CodexGitInfo>,
}

#[derive(Deserialize)]
pub struct CodexGitInfo {
    pub branch: Option<String>,
    pub repository_url: Option<String>,
}

#[derive(Deserialize)]
pub struct CodexTurnContext {
    pub model: Option<String>,
    pub collaboration_mode: Option<CodexCollaborationMode>,
}

#[derive(Deserialize)]
pub struct CodexCollaborationMode {
    pub mode: Option<String>,
}

#[derive(Deserialize)]
pub struct CodexTokenUsageInfo {
    pub input_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}
