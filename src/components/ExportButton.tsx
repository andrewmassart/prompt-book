import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import type { Session } from "../lib/types";

interface ExportButtonProps {
  session: Session;
}

const styles = {
  button: {
    background: "none",
    border: "1px solid var(--border-color)",
    color: "var(--text-secondary)",
    padding: "6px 14px",
    borderRadius: "6px",
    cursor: "pointer",
    fontSize: "0.8em",
    fontWeight: 500,
    transition: "border-color 0.15s, color 0.15s",
    flexShrink: 0,
  },
};

export function ExportButton({ session }: Readonly<ExportButtonProps>) {
  const [exporting, setExporting] = useState(false);

  const handleExport = async () => {
    try {
      const shortId = session.id.split("-")[0];
      const outputPath = await save({
        defaultPath: `prompt-book-${shortId}.html`,
        filters: [{ name: "HTML", extensions: ["html"] }],
      });

      if (!outputPath) return;

      setExporting(true);
      await invoke("export_html", { session, outputPath });
    } catch (err) {
      console.error("Export failed:", err);
    } finally {
      setExporting(false);
    }
  };

  return (
    <button
      style={styles.button}
      onClick={handleExport}
      disabled={exporting}
      onMouseEnter={(e) => {
        e.currentTarget.style.borderColor = "var(--accent)";
        e.currentTarget.style.color = "var(--accent)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.borderColor = "var(--border-color)";
        e.currentTarget.style.color = "var(--text-secondary)";
      }}
    >
      {exporting ? "Exporting..." : "Export HTML"}
    </button>
  );
}
