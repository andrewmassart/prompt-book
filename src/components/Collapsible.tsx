import { useState, type ReactNode } from "react";

interface CollapsibleProps {
  maxHeight: number;
  isLong: boolean;
  fadeBg?: string;
  buttonColor?: string;
  children: ReactNode;
}

const styles = {
  wrapper: {
    overflow: "hidden" as const,
    position: "relative" as const,
    transition: "max-height 0.2s ease",
  },
  fade: {
    position: "absolute" as const,
    bottom: 0,
    left: 0,
    right: 0,
    height: "60px",
    pointerEvents: "none" as const,
  },
  button: {
    background: "none",
    border: "none",
    cursor: "pointer",
    fontSize: "0.8em",
    padding: "6px 0 0",
  },
};

export function Collapsible({
  maxHeight,
  isLong,
  fadeBg = "var(--bg-secondary)",
  buttonColor = "var(--accent)",
  children,
}: Readonly<CollapsibleProps>) {
  const [expanded, setExpanded] = useState(!isLong);

  return (
    <>
      <div style={{
        ...styles.wrapper,
        maxHeight: expanded ? "none" : `${maxHeight}px`,
      }}>
        {children}
        {!expanded && (
          <div style={{ ...styles.fade, background: `linear-gradient(transparent, ${fadeBg})` }} />
        )}
      </div>
      {isLong && (
        <button style={{ ...styles.button, color: buttonColor }} onClick={() => setExpanded(!expanded)}>
          {expanded ? "Show less" : "Show more"}
        </button>
      )}
    </>
  );
}
