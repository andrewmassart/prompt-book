import { useState } from "react";

interface ToolCallBlockProps {
  block: {
    type: "tool_use";
    toolName: string;
    input: unknown;
    output?: string;
    durationMs?: number;
  };
}

const COLLAPSED_HEIGHT = "300px";

const styles = {
  container: {
    margin: "8px 0",
    border: "1px solid var(--border-color)",
    borderRadius: "8px",
    overflow: "hidden",
  },
  header: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    padding: "8px 12px",
    background: "var(--bg-input)",
    cursor: "pointer",
    userSelect: "none" as const,
  },
  arrow: {
    fontSize: "0.7em",
    color: "var(--text-muted)",
    transition: "transform 0.15s",
    width: "12px",
  },
  toolName: {
    fontWeight: 600,
    color: "var(--role-tool)",
    fontSize: "0.9em",
    fontFamily: "var(--font-mono)",
  },
  duration: {
    fontSize: "0.8em",
    color: "var(--text-secondary)",
    marginLeft: "auto",
  },
  body: {
    padding: "10px 12px",
    background: "var(--bg-input)",
    borderTop: "1px solid var(--border-subtle)",
  },
  label: {
    fontSize: "0.7em",
    color: "var(--text-muted)",
    textTransform: "uppercase" as const,
    marginBottom: "4px",
    marginTop: "8px",
  },
  pre: {
    fontFamily: "var(--font-mono)",
    fontSize: "0.8em",
    color: "var(--text-secondary)",
    whiteSpace: "pre-wrap" as const,
    wordBreak: "break-all" as const,
    lineHeight: 1.5,
    overflow: "auto" as const,
    transition: "max-height 0.2s ease",
  },
  expandBtn: {
    background: "none",
    border: "none",
    color: "var(--accent)",
    cursor: "pointer",
    fontSize: "0.8em",
    padding: "4px 0",
    marginTop: "4px",
  },
};

function formatToolDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(1)} s`;
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.round(seconds % 60);
  return `${minutes} m ${remainingSeconds} s`;
}

export function ToolCallBlock({ block }: ToolCallBlockProps) {
  const [open, setOpen] = useState(false);
  const [expanded, setExpanded] = useState(false);

  const inputStr =
    typeof block.input === "string"
      ? block.input
      : JSON.stringify(block.input, null, 2);

  const inputIsLarge = inputStr.split("\n").length > 15;
  const outputIsLarge = (block.output?.split("\n").length ?? 0) > 15;

  return (
    <div style={styles.container}>
      <div style={styles.header} onClick={() => setOpen(!open)}>
        <span style={{ ...styles.arrow, transform: open ? "rotate(90deg)" : "none" }}>
          &#9654;
        </span>
        <span style={styles.toolName}>{block.toolName}</span>
        {block.durationMs !== undefined && (
          <span style={styles.duration}>{formatToolDuration(block.durationMs)}</span>
        )}
      </div>
      {open && (
        <div style={styles.body}>
          <div style={styles.label}>Input</div>
          <pre style={{
            ...styles.pre,
            maxHeight: expanded ? "none" : COLLAPSED_HEIGHT,
          }}>
            {inputStr}
          </pre>
          {block.output && (
            <>
              <div style={{ ...styles.label, marginTop: "12px" }}>Output</div>
              <pre style={{
                ...styles.pre,
                maxHeight: expanded ? "none" : COLLAPSED_HEIGHT,
              }}>
                {block.output}
              </pre>
            </>
          )}
          {(inputIsLarge || outputIsLarge) && (
            <button
              style={styles.expandBtn}
              onClick={() => setExpanded(!expanded)}
            >
              {expanded ? "Collapse" : "Show full content"}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
