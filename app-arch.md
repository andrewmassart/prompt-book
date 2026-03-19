# Session Lens — Architecture

A Tauri v2 app for viewing Claude Code and Copilot CLI session transcripts.

---

## Project Structure

```
session-lens/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json          # Tauri v2 permissions
│   └── src/
│       ├── main.rs
│       ├── lib.rs                # Tauri command registrations
│       ├── commands/
│       │   ├── mod.rs
│       │   ├── discover.rs       # Auto-discover sessions
│       │   ├── parse.rs          # Parse JSONL → normalized model
│       │   └── export.rs         # Export annotated HTML
│       ├── parser/
│       │   ├── mod.rs
│       │   ├── claude.rs         # Claude Code JSONL parser
│       │   ├── copilot.rs        # Copilot CLI events.jsonl parser
│       │   └── detect.rs         # Format auto-detection
│       └── model/
│           ├── mod.rs
│           └── session.rs        # Normalized data model
├── src/
│   ├── main.tsx
│   ├── App.tsx
│   ├── components/
│   │   ├── DropZone.tsx          # Drag-and-drop JSONL intake
│   │   ├── SessionList.tsx       # Sidebar: discovered/loaded sessions
│   │   ├── SessionView.tsx       # Main content: rendered transcript
│   │   ├── MessageBubble.tsx     # Individual turn rendering
│   │   ├── ToolCallBlock.tsx     # Collapsible tool call display
│   │   ├── ThinkingBlock.tsx     # Collapsible thinking/reasoning
│   │   └── ExportButton.tsx      # HTML export trigger
│   ├── hooks/
│   │   ├── useSession.ts         # Load/parse session via Tauri IPC
│   │   └── useDiscover.ts        # Auto-discovery hook
│   ├── lib/
│   │   └── types.ts              # TypeScript mirror of Rust model
│   └── styles/
│       └── index.css
├── package.json
├── tsconfig.json
└── vite.config.ts
```

---

## Normalized Data Model (Rust)

The core insight: Claude Code and Copilot CLI have different JSONL schemas,
but both represent the same fundamental thing — a conversation with interleaved
tool use. The Rust parser normalizes both into a single model that the frontend
consumes.

```rust
// src-tauri/src/model/session.rs

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Which CLI tool produced this session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionSource {
    ClaudeCode,
    CopilotCli,
}

/// Top-level session container
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub source: SessionSource,
    pub source_path: PathBuf,
    pub title: Option<String>,          // Summary or first user message
    pub model: Option<String>,          // Primary model used
    pub started_at: Option<String>,     // ISO 8601
    pub messages: Vec<Message>,
    pub token_usage: Option<TokenUsage>,
    pub metadata: SessionMetadata,
}

/// A single conversational turn
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,                     // UUID from source, or generated
    pub role: Role,
    pub timestamp: Option<String>,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

/// Content within a message — text, tool calls, thinking, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        tool_name: String,
        input: serde_json::Value,       // Preserve raw input
        output: Option<String>,         // Tool result if available
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
        /// Base64 or path — for display purposes only
        source: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadata {
    pub working_directory: Option<String>,
    pub git_branch: Option<String>,
    pub slug: Option<String>,           // Claude Code conversation slug
}
```

---

## Tauri IPC Commands

```rust
// src-tauri/src/commands/discover.rs

#[tauri::command]
pub async fn discover_sessions() -> Result<Vec<SessionSummary>, String> {
    // Scan ~/.claude/projects/**/*.jsonl
    // Scan ~/.copilot/session-state/*/events.jsonl
    // Return lightweight summaries (no full message parsing)
}

// src-tauri/src/commands/parse.rs

#[tauri::command]
pub async fn parse_session(path: String) -> Result<Session, String> {
    // Auto-detect format, parse, normalize
}

#[tauri::command]
pub async fn parse_dropped_file(path: String) -> Result<Session, String> {
    // Same as above but for drag-and-drop paths
}

// src-tauri/src/commands/export.rs

#[tauri::command]
pub async fn export_html(session: Session, output_path: String) -> Result<(), String> {
    // Render Session into self-contained HTML
    // Inline all CSS/JS — single file output
}
```

### SessionSummary (for the sidebar — avoids parsing full sessions upfront)

```rust
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
```

---

## TypeScript Mirror Types

```typescript
// src/lib/types.ts

export type SessionSource = "claude_code" | "copilot_cli";
export type Role = "user" | "assistant" | "system" | "tool";

export type ContentBlock =
  | { type: "text"; text: string }
  | { type: "tool_use"; toolName: string; input: unknown; output?: string; durationMs?: number }
  | { type: "thinking"; text: string }
  | { type: "code_block"; language?: string; code: string }
  | { type: "image"; source: string };

export interface Message {
  id: string;
  role: Role;
  timestamp?: string;
  content: ContentBlock[];
}

export interface TokenUsage {
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens?: number;
  cacheWriteTokens?: number;
}

export interface SessionMetadata {
  workingDirectory?: string;
  gitBranch?: string;
  slug?: string;
}

export interface Session {
  id: string;
  source: SessionSource;
  sourcePath: string;
  title?: string;
  model?: string;
  startedAt?: string;
  messages: Message[];
  tokenUsage?: TokenUsage;
  metadata: SessionMetadata;
}

export interface SessionSummary {
  id: string;
  source: SessionSource;
  path: string;
  title?: string;
  startedAt?: string;
  messageCount: number;
  model?: string;
}
```

---

## Parsing Strategy

### Claude Code (`~/.claude/projects/<path>/<uuid>.jsonl`)

Each line is a JSON object. Key fields to handle:

| `type` field        | Maps to                        | Notes                                    |
|---------------------|--------------------------------|------------------------------------------|
| `"user"`            | `Message { role: User }`       | `.message.content` is the user text      |
| `"assistant"`       | `Message { role: Assistant }`  | `.message.content[]` has text, tool_use, thinking blocks |
| `"tool_result"`     | Attach to prior `ToolUse`      | Match via `tool_use_id`                  |
| `"summary"`         | Extract as session title       | Skip `isCompactSummary: true` records    |
| `"system"`          | `Message { role: System }`     | Usually init/config, can collapse        |

**Continuation handling**: If the first record's `sessionId` doesn't match the
filename UUID, those prefix records are duplicates from a parent session.
Only include records whose `sessionId` matches the filename.

### Copilot CLI (`~/.copilot/session-state/<id>/events.jsonl`)

Flat event stream. Simpler structure but different shape:

| Event pattern       | Maps to                        | Notes                                    |
|---------------------|--------------------------------|------------------------------------------|
| User prompt event   | `Message { role: User }`       | Look for user-role content               |
| Assistant response  | `Message { role: Assistant }`  | May contain tool calls inline            |
| Tool execution      | `ContentBlock::ToolUse`        | Attach to the assistant turn that invoked it |

Also read `workspace.yaml` alongside `events.jsonl` to populate `SessionMetadata`.

### Format Detection (`parser/detect.rs`)

```rust
pub fn detect_format(path: &Path) -> Result<SessionSource, ParseError> {
    // 1. Path heuristic: contains ".claude/projects" → ClaudeCode
    //                    contains ".copilot/session-state" → CopilotCli
    // 2. Content heuristic: read first line, check for known fields
    //    - Has "type": "user"/"assistant" with "message" → ClaudeCode
    //    - Has event-stream structure → CopilotCli
}
```

---

## Tauri v2 Capabilities

```json
// src-tauri/capabilities/default.json
{
  "identifier": "default",
  "description": "Default capabilities for Session Lens",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "dialog:default",
    "fs:default",
    "fs:allow-read",
    "fs:allow-exists",
    "shell:allow-open"
  ]
}
```

---

## Export Strategy

The HTML export should produce a **single self-contained file** — all CSS and
JS inlined, no external dependencies. This is what makes it shareable via
Slack/email/Teams without hosting anything.

The export template lives as a Rust `include_str!` embedded asset. At export
time, the session data is serialized to JSON and injected into a `<script>`
tag. The template contains a minimal renderer (vanilla JS, no framework needed
for the static output) that reads the embedded data and builds the DOM.

This means the interactive Tauri app uses React for its full UI, but the
exported HTML is intentionally framework-free for portability.

---

## Build & Distribution

```bash
# Dev
cargo tauri dev

# Build for distribution
cargo tauri build
# Produces: src-tauri/target/release/bundle/
#   - .msi (Windows, Intune-friendly)
#   - .dmg / .app (macOS)
#   - .deb / .AppImage (Linux)
```

The MSI output is directly deployable via Intune with no additional packaging.