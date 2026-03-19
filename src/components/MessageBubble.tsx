import { useState } from "react";
import type { Message, ContentBlock, MessageMode } from "../lib/types";
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
  badge: {
    fontSize: "0.6em",
    fontWeight: 700,
    padding: "1px 5px",
    borderRadius: "3px",
    letterSpacing: "0.05em",
    lineHeight: 1.4,
  },
  contentWrapper: {
    overflow: "hidden" as const,
    position: "relative" as const,
    transition: "max-height 0.2s ease",
  },
  fade: {
    position: "absolute" as const,
    bottom: 0,
    left: 0,
    right: 0,
    height: "60px",
    background: "linear-gradient(transparent, var(--bg-secondary))",
    pointerEvents: "none" as const,
  },
  expandBtn: {
    background: "none",
    border: "none",
    cursor: "pointer",
    fontSize: "0.8em",
    padding: "6px 0 0",
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

function formatTime(iso?: string): string {
  if (!iso) return "";
  try {
    return new Date(iso).toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  } catch {
    return "";
  }
}

function estimateLineCount(content: ContentBlock[]): number {
  let lines = 0;
  for (const block of content) {
    switch (block.type) {
      case "text":
        lines += block.text.split("\n").length;
        break;
      case "code_block":
        lines += block.code.split("\n").length;
        break;
      case "tool_use":
      case "thinking":
        lines += 2;
        break;
      default:
        lines += 1;
    }
  }
  return lines;
}

function renderBlock(block: ContentBlock, index: number) {
  switch (block.type) {
    case "text":
      return (
        <div key={index} style={styles.textContent}>
          {block.text}
        </div>
      );
    case "tool_use":
      return <ToolCallBlock key={index} block={block} />;
    case "thinking":
      return <ThinkingBlock key={index} text={block.text} />;
    case "code_block":
      return (
        <div key={index}>
          {block.language && <div style={styles.codeLang}>{block.language}</div>}
          <pre style={styles.codeBlock}>{block.code}</pre>
        </div>
      );
    case "image":
      return <img key={index} src={block.source} alt="embedded" style={styles.image} />;
    default:
      return null;
  }
}

export function MessageBubble({ message }: MessageBubbleProps) {
  const color = roleColors[message.role] || "var(--border-color)";
  const modeLabel = modeLabels[message.mode];
  const modeColor = modeColors[message.mode];
  const isLong = estimateLineCount(message.content) > LINE_THRESHOLD;
  const [expanded, setExpanded] = useState(!isLong);

  const bubbleStyle = {
    ...styles.bubble,
    borderLeftColor: message.isAgent ? "var(--agent-border)" : color,
    ...(message.isAgent ? styles.agentBubble : {}),
    ...(message.isMeta ? styles.metaBubble : {}),
    cursor: isLong && expanded ? "pointer" : undefined,
  };

  return (
    <div
      style={bubbleStyle}
      onClick={isLong && expanded ? () => setExpanded(false) : undefined}
    >
      <div style={styles.headerRow}>
        <span style={{ ...styles.roleLabel, color: message.isAgent ? "var(--agent-border)" : color }}>
          {message.isAgent ? `agent / ${message.role}` : message.role}
        </span>
        {modeLabel && modeColor && (
          <span
            style={{
              ...styles.badge,
              color: modeColor,
              border: `1px solid ${modeColor}`,
            }}
          >
            {modeLabel}
          </span>
        )}
        {message.isMeta && (
          <span
            style={{
              ...styles.badge,
              color: "var(--text-muted)",
              border: "1px solid var(--text-muted)",
            }}
          >
            META
          </span>
        )}
        {message.timestamp && (
          <span style={styles.timestamp}>{formatTime(message.timestamp)}</span>
        )}
      </div>
      <div style={{
        ...styles.contentWrapper,
        maxHeight: expanded ? "none" : `${COLLAPSED_HEIGHT}px`,
      }}>
        {message.content.map((block, i) => renderBlock(block, i))}
        {!expanded && <div style={styles.fade} />}
      </div>
      {isLong && !expanded && (
        <button
          style={{ ...styles.expandBtn, color }}
          onClick={() => setExpanded(true)}
        >
          Show more
        </button>
      )}
    </div>
  );
}
