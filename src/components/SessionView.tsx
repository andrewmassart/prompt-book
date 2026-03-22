import { useRef, useState, useEffect } from "react";
import type { Session } from "../lib/types";
import { formatTokens } from "../lib/formatters";
import { hasVisibleContent } from "../lib/content";
import { MessageBubble } from "./MessageBubble";

interface SessionViewProps {
  session: Session | null;
  loading: boolean;
  error: string | null;
}

const styles = {
  container: {
    flex: 1,
    display: "flex",
    flexDirection: "column" as const,
    height: "100%",
    overflow: "hidden",
    position: "relative" as const,
  },
  header: {
    padding: "10px 16px",
    height: "60px",
    borderBottom: "1px solid var(--border-color)",
    background: "var(--bg-secondary)",
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    flexShrink: 0,
  },
  headerLeft: {
    flex: 1,
    minWidth: 0,
  },
  title: {
    fontSize: "1.05em",
    fontWeight: 600,
    color: "var(--text-primary)",
    overflow: "hidden" as const,
    textOverflow: "ellipsis" as const,
    whiteSpace: "nowrap" as const,
  },
  meta: {
    fontSize: "0.8em",
    color: "var(--text-muted)",
    marginTop: "2px",
    display: "flex",
    gap: "12px",
  },
  messages: {
    flex: 1,
    overflowY: "auto" as const,
    padding: "16px 20px",
  },
  empty: {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    height: "100%",
    color: "var(--text-muted)",
    fontSize: "1.1em",
    flexDirection: "column" as const,
    gap: "8px",
  },
  emptySubtext: {
    fontSize: "0.8em",
    color: "var(--text-muted)",
  },
  loading: {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    height: "100%",
    color: "var(--text-secondary)",
    fontSize: "1em",
  },
  error: {
    padding: "20px",
    color: "var(--error)",
  },
  fab: {
    position: "absolute" as const,
    bottom: "20px",
    right: "20px",
    width: "48px",
    height: "48px",
    borderRadius: "50%",
    background: "var(--accent)",
    color: "#fff",
    border: "none",
    cursor: "pointer",
    fontSize: "1.2em",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    boxShadow: "0 2px 8px rgba(0,0,0,0.4)",
    transition: "opacity 0.15s",
  },
};

export function SessionView({ session, loading, error }: Readonly<SessionViewProps>) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [showFab, setShowFab] = useState(false);

  const handleScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    setShowFab(distanceFromBottom > 0);
  };

  const scrollToBottom = () => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: "smooth" });
  };

  useEffect(() => {
    const el = scrollRef.current;
    if (el) {
      setShowFab(el.scrollHeight > el.clientHeight);
    } else {
      setShowFab(false);
    }
  }, [session?.id]);

  if (loading) {
    return (
      <div style={styles.container}>
        <div style={styles.loading}>Loading session...</div>
      </div>
    );
  }

  if (error) {
    return (
      <div style={styles.container}>
        <div style={styles.error}>{error}</div>
      </div>
    );
  }

  if (!session) {
    return (
      <div style={styles.container}>
        <div style={styles.empty}>
          <div>Select a session to view</div>
          <div style={styles.emptySubtext}>or drag and drop a .jsonl file</div>
        </div>
      </div>
    );
  }

  const visibleMessages = session.messages.filter(hasVisibleContent);

  return (
    <div style={styles.container}>
      <div style={styles.header}>
        <div style={styles.headerLeft}>
          <div style={styles.title}>{session.title || "Untitled Session"}</div>
          <div style={styles.meta}>
            {session.model && <span>{session.model}</span>}
            {session.startedAt && (
              <span>{new Date(session.startedAt).toLocaleString()}</span>
            )}
            <span>{visibleMessages.length} messages</span>
            {session.tokenUsage && (
              <span>
                {formatTokens(session.tokenUsage.inputTokens)} in /{" "}
                {formatTokens(session.tokenUsage.outputTokens)} out
              </span>
            )}
          </div>
        </div>
      </div>
      <div style={styles.messages} ref={scrollRef} onScroll={handleScroll}>
        {visibleMessages.map((msg) => (
          <MessageBubble key={msg.id} message={msg} />
        ))}
      </div>
      {showFab && (
        <button
          className="fab-hover"
          style={styles.fab}
          onClick={scrollToBottom}
        >
          &#8595;
        </button>
      )}
    </div>
  );
}
