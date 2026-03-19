import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { useDiscover } from "./hooks/useDiscover";
import { useSession } from "./hooks/useSession";
import { SessionList } from "./components/SessionList";
import { SessionView } from "./components/SessionView";
import { DropZone, openJsonlFile } from "./components/DropZone";
import type { Session, SessionSummary } from "./lib/types";

const styles = {
  layout: {
    display: "flex",
    height: "100vh",
    width: "100vw",
    overflow: "hidden",
  },
  sidebar: {
    width: "var(--sidebar-width)",
    flexShrink: 0,
    height: "100%",
  },
  main: {
    flex: 1,
    height: "100%",
    overflow: "hidden",
  },
};

function sessionToSummary(s: Session): SessionSummary {
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

function App() {
  const { sessions, loading: discoverLoading, error: discoverError, refresh, addSession } = useDiscover();
  const { session, loading: sessionLoading, error: sessionError, loadSession, loadDroppedFile, loadContent, loadById } = useSession();
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const handleSelect = useCallback(
    (summary: SessionSummary) => {
      setSelectedId(summary.id);
      if (!loadById(summary.id)) {
        loadSession(summary.path);
      }
    },
    [loadById, loadSession],
  );

  const handleSessionLoaded = useCallback(
    (loaded: Session) => {
      setSelectedId(loaded.id);
      addSession(sessionToSummary(loaded));
    },
    [addSession],
  );

  const handleFileContent = useCallback(
    async (filename: string, content: string) => {
      setSelectedId(null);
      const result = await loadContent(filename, content);
      if (result) handleSessionLoaded(result);
    },
    [loadContent, handleSessionLoaded],
  );

  const handleOpenFile = useCallback(async () => {
    const path = await openJsonlFile();
    if (path) {
      setSelectedId(null);
      const result = await loadDroppedFile(path);
      if (result) handleSessionLoaded(result);
    }
  }, [loadDroppedFile, handleSessionLoaded]);

  const handleExport = useCallback(async () => {
    if (!session) return;
    const outputPath = await save({
      defaultPath: `${session.title || "session"}.html`,
      filters: [{ name: "HTML", extensions: ["html"] }],
    });
    if (!outputPath) return;
    await invoke("export_html", { session, outputPath });
  }, [session]);

  return (
    <DropZone onFileContent={handleFileContent}>
      <div style={styles.layout}>
        <div style={styles.sidebar}>
          <SessionList
            sessions={sessions}
            loading={discoverLoading}
            error={discoverError}
            selectedId={selectedId}
            onSelect={handleSelect}
            onRefresh={refresh}
            onOpenFile={handleOpenFile}
            onExport={session ? handleExport : null}
          />
        </div>
        <div style={styles.main}>
          <SessionView
            session={session}
            loading={sessionLoading}
            error={sessionError}
          />
        </div>
      </div>
    </DropZone>
  );
}

export default App;
