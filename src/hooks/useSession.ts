import { useState, useRef } from "react";
import type { Session } from "../lib/types";
import { invokeCommand } from "../lib/commands";
import { errorMessage } from "../lib/error";

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

  const cacheAndSet = (result: Session) => {
    cache.current.set(result.id, result);
    setSession(result);
  };

  const loadById = (id: string): boolean => {
    const cached = cache.current.get(id);
    if (cached) {
      setSession(cached);
      setError(null);
      return true;
    }
    return false;
  };

  const loadSession = async (path: string) => {
    setLoading(true);
    setError(null);
    try {
      const result = await invokeCommand("parse_session", { path });
      cacheAndSet(result);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setLoading(false);
    }
  };

  const loadDroppedFile = async (path: string): Promise<Session | null> => {
    setLoading(true);
    setError(null);
    try {
      const result = await invokeCommand("parse_dropped_file", { path });
      cacheAndSet(result);
      return result;
    } catch (err) {
      setError(errorMessage(err));
      return null;
    } finally {
      setLoading(false);
    }
  };

  const loadContent = async (filename: string, content: string): Promise<Session | null> => {
    setLoading(true);
    setError(null);
    try {
      const result = await invokeCommand("parse_content", { filename, content });
      cacheAndSet(result);
      return result;
    } catch (err) {
      setError(errorMessage(err));
      return null;
    } finally {
      setLoading(false);
    }
  };

  return { session, loading, error, loadSession, loadDroppedFile, loadContent, loadById };
}
