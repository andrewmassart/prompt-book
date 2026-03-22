import type { Session, SessionSummary } from "./types";

export function sessionToSummary(s: Session): SessionSummary {
  return {
    id: s.id,
    source: s.source,
    path: s.sourcePath,
    title: s.title,
    startedAt: s.startedAt,
    messageCount: s.messages.length,
    model: s.model,
  };
}
