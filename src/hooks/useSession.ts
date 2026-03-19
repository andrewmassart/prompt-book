import { useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Session } from "../lib/types";

interface UseSessionResult {
  session: Session | null;
  loading: boolean;
  error: string | null;
  loadSession: (path: string) => void;
  loadDroppedFile: (path: string) => Promise<Session | null>;
  loadContent: (filename: string, content: string) => Promise<Session | null>;
  loadById: (id: string) => boolean;
}

export function useSession(): UseSessionResult {
  const [session, setSession] = useState<Session | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const cache = useRef<Map<string, Session>>(new Map());

  const cacheAndSet = useCallback((result: Session) => {
    cache.current.set(result.id, result);
    setSession(result);
  }, []);

  const loadById = useCallback((id: string): boolean => {
    const cached = cache.current.get(id);
    if (cached) {
      setSession(cached);
      setError(null);
      return true;
    }
    return false;
  }, []);

  const loadSession = useCallback(async (path: string) => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<Session>("parse_session", { path });
      cacheAndSet(result);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [cacheAndSet]);

  const loadDroppedFile = useCallback(async (path: string): Promise<Session | null> => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<Session>("parse_dropped_file", { path });
      cacheAndSet(result);
      return result;
    } catch (err) {
      setError(String(err));
      return null;
    } finally {
      setLoading(false);
    }
  }, [cacheAndSet]);

  const loadContent = useCallback(async (filename: string, content: string): Promise<Session | null> => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<Session>("parse_content", { filename, content });
      cacheAndSet(result);
      return result;
    } catch (err) {
      setError(String(err));
      return null;
    } finally {
      setLoading(false);
    }
  }, [cacheAndSet]);

  return { session, loading, error, loadSession, loadDroppedFile, loadContent, loadById };
}
