import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SessionSummary } from "../lib/types";

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
      const result = await invoke<SessionSummary[]>("discover_sessions");
      setSessions(result);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchSessions();
  }, []);

  const addSession = useCallback((summary: SessionSummary) => {
    setSessions((prev) => {
      const filtered = prev.filter((s) => s.id !== summary.id);
      return [summary, ...filtered];
    });
  }, []);

  return { sessions, loading, error, refresh: fetchSessions, addSession };
}
