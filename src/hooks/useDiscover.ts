import { useState, useEffect } from "react";
import type { SessionSummary } from "../lib/types";
import { invokeCommand } from "../lib/commands";
import { errorMessage } from "../lib/error";

interface UseDiscoverResult {
  sessions: SessionSummary[];
  loading: boolean;
  error: string | null;
  refresh: () => void;
  addSession: (summary: SessionSummary) => void;
}

export function useDiscover(): UseDiscoverResult {
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchSessions = async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invokeCommand("discover_sessions", {});
      setSessions(result);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchSessions();
  }, []);

  const addSession = (summary: SessionSummary) => {
    setSessions((prev) => {
      const filtered = prev.filter((s) => s.id !== summary.id);
      return [summary, ...filtered];
    });
  };

  return { sessions, loading, error, refresh: fetchSessions, addSession };
}
