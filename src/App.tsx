import { useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { invokeCommand } from "./lib/commands";
import { useDiscover } from "./hooks/useDiscover";
import { useSession } from "./hooks/useSession";
import { SessionList } from "./components/SessionList";
import { SessionView } from "./components/SessionView";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { DropZone, openJsonlFile } from "./components/DropZone";
import type { Session, SessionSummary } from "./lib/types";
import { sessionToSummary } from "./lib/transforms";

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

function App() {
  const { sessions, loading: discoverLoading, error: discoverError, refresh, addSession } = useDiscover();
  const { session, loading: sessionLoading, error: sessionError, loadSession, loadDroppedFile, loadContent, loadById } = useSession();
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const handleSelect = (summary: SessionSummary) => {
    setSelectedId(summary.id);
    if (!loadById(summary.id)) {
      loadSession(summary.path);
    }
  };

  const handleSessionLoaded = (loaded: Session) => {
    setSelectedId(loaded.id);
    addSession(sessionToSummary(loaded));
  };

  const handleFileContent = async (filename: string, content: string) => {
    setSelectedId(null);
    const result = await loadContent(filename, content);
    if (result) handleSessionLoaded(result);
  };

  const handleOpenFile = async () => {
    const path = await openJsonlFile();
    if (path) {
      setSelectedId(null);
      const result = await loadDroppedFile(path);
      if (result) handleSessionLoaded(result);
    }
  };

  const handleExport = async () => {
    if (!session) return;
    const outputPath = await save({
      defaultPath: `${session.title || "session"}.html`,
      filters: [{ name: "HTML", extensions: ["html"] }],
    });
    if (!outputPath) return;
    await invokeCommand("export_html", { session, outputPath });
  };

  return (
    <DropZone onFileContent={handleFileContent}>
      <div style={styles.layout}>
        <div style={styles.sidebar}>
          <SessionList
            sessions={sessions.filter((s) => s.messageCount > 0)}
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
          <ErrorBoundary>
            <SessionView
              session={session}
              loading={sessionLoading}
              error={sessionError}
            />
          </ErrorBoundary>
        </div>
      </div>
    </DropZone>
  );
}

export default App;
