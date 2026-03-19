import { useState } from "react";

interface ThinkingBlockProps {
  text: string;
}

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
  label: {
    fontStyle: "italic",
    color: "var(--text-muted)",
    fontSize: "0.85em",
  },
  body: {
    padding: "10px 12px",
    background: "var(--bg-input)",
    borderTop: "1px solid var(--border-subtle)",
    fontStyle: "italic",
    color: "var(--text-muted)",
    fontSize: "0.85em",
    whiteSpace: "pre-wrap" as const,
    wordWrap: "break-word" as const,
    lineHeight: 1.6,
    maxHeight: "400px",
    overflow: "auto" as const,
  },
};

export function ThinkingBlock({ text }: Readonly<ThinkingBlockProps>) {
  const [open, setOpen] = useState(false);

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
        <span style={styles.label}>Thinking...</span>
      </button>
      {open && <div style={styles.body}>{text}</div>}
    </div>
  );
}
