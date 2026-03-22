import type { Message, ContentBlock, MessageMode } from "../lib/types";
import { formatDuration, formatTime } from "../lib/formatters";
import { estimateLineCount } from "../lib/content";
import { Collapsible } from "./Collapsible";
import { ToolCallBlock } from "./ToolCallBlock";
import { ThinkingBlock } from "./ThinkingBlock";

interface MessageBubbleProps {
  message: Message;
}

const COLLAPSED_HEIGHT = 200;
const LINE_THRESHOLD = 10;

const roleColors: Record<string, string> = {
  user: "var(--role-user)",
  assistant: "var(--role-assistant)",
  system: "var(--role-system)",
  tool: "var(--role-tool)",
};

const modeLabels: Record<MessageMode, string | null> = {
  normal: null,
  plan: "PLAN",
  auto: "AUTO",
};

const modeColors: Record<MessageMode, string | null> = {
  normal: null,
  plan: "var(--mode-plan)",
  auto: "var(--mode-auto)",
};

const styles = {
  bubble: {
    marginBottom: "8px",
    padding: "12px 16px",
    borderRadius: "10px",
    borderLeft: "3px solid",
    background: "var(--bg-secondary)",
    position: "relative" as const,
  },
  agentBubble: {
    marginLeft: "20px",
    borderLeftStyle: "dashed" as const,
    opacity: 0.85,
  },
  metaBubble: {
    opacity: 0.5,
    fontSize: "0.85em",
  },
  headerRow: {
    display: "flex",
    alignItems: "center",
    gap: "6px",
    marginBottom: "6px",
  },
  roleLabel: {
    fontSize: "0.7em",
    textTransform: "uppercase" as const,
    fontWeight: 600,
    letterSpacing: "0.06em",
  },
  timestamp: {
    fontSize: "0.85em",
    color: "var(--text-secondary)",
    marginLeft: "8px",
    fontWeight: 400 as const,
    letterSpacing: "normal" as const,
    textTransform: "none" as const,
  },
  duration: {
    fontSize: "0.75em",
    color: "var(--text-muted)",
    marginLeft: "auto",
  },
  badge: {
    fontSize: "0.6em",
    fontWeight: 700,
    padding: "1px 5px",
    borderRadius: "3px",
    letterSpacing: "0.05em",
    lineHeight: 1.4,
  },
  textContent: {
    whiteSpace: "pre-wrap" as const,
    wordWrap: "break-word" as const,
    lineHeight: 1.6,
  },
  codeBlock: {
    background: "var(--bg-input)",
    padding: "10px 12px",
    borderRadius: "6px",
    fontFamily: "var(--font-mono)",
    fontSize: "0.85em",
    overflow: "auto" as const,
    margin: "8px 0",
    border: "1px solid var(--border-subtle)",
    whiteSpace: "pre-wrap" as const,
  },
  codeLang: {
    fontSize: "0.7em",
    color: "var(--text-muted)",
    marginBottom: "4px",
    textTransform: "uppercase" as const,
  },
  image: {
    maxWidth: "100%",
    borderRadius: "8px",
    margin: "8px 0",
  },
};

function blockKey(block: ContentBlock, index: number): string {
  switch (block.type) {
    case "tool_use": return `tool-${block.toolName}-${index}`;
    case "text": return `text-${index}`;
    case "thinking": return `think-${index}`;
    case "code_block": return `code-${index}`;
    case "image": return `img-${index}`;
  }
}

function renderBlock(block: ContentBlock, key: string) {
  switch (block.type) {
    case "text":
      return <div key={key} style={styles.textContent}>{block.text}</div>;
    case "tool_use":
      return <ToolCallBlock key={key} block={block} />;
    case "thinking":
      return <ThinkingBlock key={key} text={block.text} />;
    case "code_block":
      return (
        <div key={key}>
          {block.language && <div style={styles.codeLang}>{block.language}</div>}
          <pre style={styles.codeBlock}>{block.code}</pre>
        </div>
      );
    case "image":
      return <img key={key} src={block.source} alt="embedded" style={styles.image} />;
  }
}

function MessageHeader({ message }: { message: Message }) {
  const color = roleColors[message.role] || "var(--border-color)";
  const modeLabel = modeLabels[message.mode];
  const modeColor = modeColors[message.mode];

  return (
    <div style={styles.headerRow}>
      <span style={{ ...styles.roleLabel, color: message.isAgent ? "var(--agent-border)" : color }}>
        {message.isAgent ? `agent / ${message.role}` : message.role}
      </span>
      {modeLabel && modeColor && (
        <span style={{ ...styles.badge, color: modeColor, border: `1px solid ${modeColor}` }}>
          {modeLabel}
        </span>
      )}
      {message.isMeta && (
        <span style={{ ...styles.badge, color: "var(--text-muted)", border: "1px solid var(--text-muted)" }}>
          META
        </span>
      )}
      {message.timestamp && (
        <span style={styles.timestamp}>{formatTime(message.timestamp)}</span>
      )}
      {message.durationMs != null && (
        <span style={styles.duration}>{formatDuration(message.durationMs)}</span>
      )}
    </div>
  );
}

function MessageContent({ message, color }: { message: Message; color: string }) {
  const isLong = estimateLineCount(message.content) > LINE_THRESHOLD;

  return (
    <Collapsible maxHeight={COLLAPSED_HEIGHT} isLong={isLong} buttonColor={color}>
      {message.content.map((block, i) => renderBlock(block, blockKey(block, i)))}
    </Collapsible>
  );
}

export function MessageBubble({ message }: Readonly<MessageBubbleProps>) {
  const color = roleColors[message.role] || "var(--border-color)";

  const bubbleStyle = {
    ...styles.bubble,
    borderLeftColor: message.isAgent ? "var(--agent-border)" : color,
    ...(message.isAgent ? styles.agentBubble : {}),
    ...(message.isMeta ? styles.metaBubble : {}),
  };

  return (
    <div style={bubbleStyle}>
      <MessageHeader message={message} />
      <MessageContent message={message} color={color} />
    </div>
  );
}
