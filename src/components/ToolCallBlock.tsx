import { useState } from "react";
import { formatDuration } from "../lib/formatters";
import type { ContentBlock } from "../lib/types";
import { Collapsible } from "./Collapsible";

interface ToolCallBlockProps {
  block: Extract<ContentBlock, { type: "tool_use" }>;
}

const LINE_THRESHOLD = 15;

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
    border: "none",
    width: "100%",
    font: "inherit",
    textAlign: "left" as const,
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
  },
};

export function ToolCallBlock({ block }: Readonly<ToolCallBlockProps>) {
  const [open, setOpen] = useState(false);

  const inputStr =
    typeof block.input === "string"
      ? block.input
      : JSON.stringify(block.input, null, 2);

  const inputIsLong = inputStr.split("\n").length > LINE_THRESHOLD;
  const outputIsLong = (block.output?.split("\n").length ?? 0) > LINE_THRESHOLD;

  return (
    <div style={styles.container}>
      <button
        type="button"
        style={styles.header}
        onClick={() => setOpen(!open)}
      >
        <span style={{ ...styles.arrow, transform: open ? "rotate(90deg)" : "none" }}>
          &#9654;
        </span>
        <span style={styles.toolName}>{block.toolName}</span>
        {block.durationMs != null && (
          <span style={styles.duration}>{formatDuration(block.durationMs)}</span>
        )}
      </button>
      {open && (
        <div style={styles.body}>
          <div style={styles.label}>Input</div>
          <Collapsible maxHeight={300} isLong={inputIsLong} fadeBg="var(--bg-input)">
            <pre style={styles.pre}>{inputStr}</pre>
          </Collapsible>
          {block.output && (
            <>
              <div style={{ ...styles.label, marginTop: "12px" }}>Output</div>
              <Collapsible maxHeight={300} isLong={outputIsLong} fadeBg="var(--bg-input)">
                <pre style={styles.pre}>{block.output}</pre>
              </Collapsible>
            </>
          )}
        </div>
      )}
    </div>
  );
}
