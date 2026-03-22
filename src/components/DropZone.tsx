import { useState, useRef, type DragEvent, type ReactNode } from "react";
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
    background: "var(--bg-overlay)",
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

export function DropZone({ onFileContent, children }: Readonly<DropZoneProps>) {
  const [dragging, setDragging] = useState(false);
  const dragCounterRef = useRef(0);

  const handleDragEnter = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragCounterRef.current++;
    if (dragCounterRef.current === 1) setDragging(true);
  };

  const handleDragLeave = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragCounterRef.current--;
    if (dragCounterRef.current === 0) setDragging(false);
  };

  const handleDragOver = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
  };

  const handleDrop = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setDragging(false);
    dragCounterRef.current = 0;

    const files = e.dataTransfer?.files;
    if (!files || files.length === 0) return;

    for (const file of Array.from(files)) {
      if (file.name.endsWith(".jsonl")) {
        file.text().then((content) => onFileContent(file.name, content)).catch(console.error);
        return;
      }
    }
  };

  return (
    <div
      role="region"
      aria-label="File drop zone"
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
  return path;
}
