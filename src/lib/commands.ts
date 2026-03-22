import { invoke } from "@tauri-apps/api/core";
import type { Session, SessionSummary } from "./types";

interface CommandMap {
  discover_sessions: { args: Record<string, never>; result: SessionSummary[] };
  parse_session: { args: { path: string }; result: Session };
  parse_dropped_file: { args: { path: string }; result: Session };
  parse_content: { args: { filename: string; content: string }; result: Session };
  export_html: { args: { session: Session; outputPath: string }; result: void };
}

type CommandName = keyof CommandMap;

export function invokeCommand<K extends CommandName>(
  cmd: K,
  args: CommandMap[K]["args"],
): Promise<CommandMap[K]["result"]> {
  return invoke<CommandMap[K]["result"]>(cmd, args);
}
