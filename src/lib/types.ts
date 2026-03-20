export type SessionSource = "claude_code" | "copilot_cli" | "codex";
export type Role = "user" | "assistant" | "system" | "tool";
export type MessageMode = "normal" | "plan" | "auto";

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
  mode: MessageMode;
  isAgent: boolean;
  isMeta: boolean;
  durationMs?: number;
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
  repository?: string;
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
