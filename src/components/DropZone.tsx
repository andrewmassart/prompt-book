import { useState, useCallback, type DragEvent, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";

interface DropZoneProps {
  onFileContent: (filename: string, content: string) => void;
  children: ReactNode;
}

const styles = {
  wrapper: {
    position: "relative" as const,
    width: "100%",
    height: "100%",
  },
  overlay: {
    position: "absolute" as const,
    inset: 0,
    background: "rgba(22, 22, 22, 0.92)",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    zIndex: 1000,
    border: "3px dashed var(--accent)",
    borderRadius: "12px",
    margin: "8px",
  },
  overlayText: {
    fontSize: "1.3em",
    color: "var(--accent)",
    fontWeight: 600,
  },
};

export function DropZone({ onFileContent, children }: DropZoneProps) {
  const [dragging, setDragging] = useState(false);
  const [, setDragCounter] = useState(0);

  const handleDragEnter = useCallback((e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setDragCounter((c) => {
      if (c === 0) setDragging(true);
      return c + 1;
    });
  }, []);

  const handleDragLeave = useCallback((e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setDragCounter((c) => {
      const next = c - 1;
      if (next === 0) setDragging(false);
      return next;
    });
  }, []);

  const handleDragOver = useCallback((e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
  }, []);

  const handleDrop = useCallback(
    (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setDragging(false);
      setDragCounter(0);

      const files = e.dataTransfer?.files;
      if (!files || files.length === 0) return;

      for (let i = 0; i < files.length; i++) {
        const file = files[i];
        if (file.name.endsWith(".jsonl")) {
          const reader = new FileReader();
          reader.onload = () => {
            if (typeof reader.result === "string") {
              onFileContent(file.name, reader.result);
            }
          };
          reader.readAsText(file);
          return;
        }
      }
    },
    [onFileContent],
  );

  return (
    <div
      style={styles.wrapper}
      onDragEnter={handleDragEnter}
      onDragLeave={handleDragLeave}
      onDragOver={handleDragOver}
      onDrop={handleDrop}
    >
      {dragging && (
        <div style={styles.overlay}>
          <span style={styles.overlayText}>Drop .jsonl file here</span>
        </div>
      )}
      {children}
    </div>
  );
}

export async function openJsonlFile(): Promise<string | null> {
  const path = await open({
    filters: [{ name: "JSONL", extensions: ["jsonl"] }],
    multiple: false,
  });
  return path as string | null;
}
