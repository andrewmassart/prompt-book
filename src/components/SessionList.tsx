import { useState } from "react";
import type { SessionSummary } from "../lib/types";
import { formatDate } from "../lib/formatters";

interface SessionListProps {
  sessions: SessionSummary[];
  loading: boolean;
  error: string | null;
  selectedId: string | null;
  onSelect: (summary: SessionSummary) => void;
  onRefresh: () => void;
  onOpenFile: () => void;
  onExport: (() => void) | null;
}

const styles = {
  container: {
    display: "flex",
    flexDirection: "column" as const,
    height: "100%",
    background: "var(--bg-secondary)",
    borderRight: "1px solid var(--border-color)",
  },
  header: {
    padding: "16px",
    height: "60px",
    borderBottom: "1px solid var(--border-color)",
    display: "flex",
    alignItems: "center",
    justifyContent: "flex-end",
    gap: "8px",
    flexShrink: 0,
  },
  title: {
    fontSize: "1.1em",
    fontWeight: 700,
    color: "var(--text-primary)",
    letterSpacing: "0.02em",
  },
  refreshBtn: {
    background: "none",
    border: "1px solid var(--border-color)",
    color: "var(--text-secondary)",
    padding: "4px 10px",
    borderRadius: "6px",
    cursor: "pointer",
    fontSize: "0.8em",
    transition: "border-color 0.15s, color 0.15s",
  },
  list: {
    flex: 1,
    overflowY: "auto" as const,
    padding: "8px",
  },
  item: {
    padding: "10px 12px",
    marginBottom: "4px",
    borderRadius: "8px",
    cursor: "pointer",
    transition: "background 0.15s",
    border: "1px solid transparent",
    width: "100%",
    textAlign: "left" as const,
    font: "inherit",
    background: "none",
    display: "block",
  },
  itemSelected: {
    background: "var(--accent-subtle)",
    border: "1px solid var(--accent-dim)",
  },
  itemHover: {
    background: "var(--bg-tertiary)",
  },
  itemTitle: {
    fontSize: "0.9em",
    fontWeight: 500,
    color: "var(--text-primary)",
    overflow: "hidden" as const,
    textOverflow: "ellipsis" as const,
    whiteSpace: "nowrap" as const,
    marginBottom: "4px",
  },
  itemMeta: {
    fontSize: "0.75em",
    color: "var(--text-secondary)",
    display: "flex",
    gap: "8px",
  },
  empty: {
    padding: "20px",
    textAlign: "center" as const,
    color: "var(--text-muted)",
    fontSize: "0.9em",
  },
  error: {
    padding: "12px",
    color: "var(--error)",
    fontSize: "0.85em",
  },
};

function HeaderButton({ onClick, disabled, label }: Readonly<{ onClick: () => void; disabled?: boolean; label: string }>) {
  return (
    <button
      className="hover-accent"
      style={styles.refreshBtn}
      onClick={onClick}
      disabled={disabled}
    >
      {label}
    </button>
  );
}

export function SessionList({
  sessions,
  loading,
  error,
  selectedId,
  onSelect,
  onRefresh,
  onOpenFile,
  onExport,
}: Readonly<SessionListProps>) {
  const [hoveredId, setHoveredId] = useState<string | null>(null);

  return (
    <div style={styles.container}>
      <div style={styles.header}>
        <HeaderButton onClick={onOpenFile} label="Open" />
        {onExport && <HeaderButton onClick={onExport} label="Export" />}
        <HeaderButton onClick={onRefresh} disabled={loading} label={loading ? "..." : "Refresh"} />
      </div>

      <div style={styles.list}>
        {error && <div style={styles.error}>{error}</div>}

        {!loading && sessions.length === 0 && !error && (
          <div style={styles.empty}>
            No sessions found.<br />
            Drop a .jsonl file or check ~/.claude/projects
          </div>
        )}

        {sessions.map((s) => (
          <button
            key={s.id}
            type="button"
            style={{
              ...styles.item,
              ...(selectedId === s.id ? styles.itemSelected : {}),
              ...(hoveredId === s.id && selectedId !== s.id ? styles.itemHover : {}),
            }}
            onClick={() => onSelect(s)}
            onMouseEnter={() => setHoveredId(s.id)}
            onMouseLeave={() => setHoveredId(null)}
          >
            <div style={styles.itemTitle}>{s.title || "Untitled"}</div>
            <div style={styles.itemMeta}>
              <span>{formatDate(s.startedAt)}</span>
              <span>{s.messageCount} msgs</span>
              {s.model && <span>{s.model.split("-").slice(0, 2).join("-")}</span>}
            </div>
          </button>
        ))}
      </div>
    </div>
  );
}
