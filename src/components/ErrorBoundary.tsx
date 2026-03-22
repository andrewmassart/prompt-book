import { Component, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

const styles = {
  container: {
    padding: "20px",
    color: "var(--error)",
    background: "var(--bg-secondary)",
    borderRadius: "8px",
    margin: "20px",
  },
  heading: {
    marginBottom: "8px",
  },
  message: {
    fontSize: "0.85em",
    whiteSpace: "pre-wrap" as const,
  },
  button: {
    marginTop: "12px",
    padding: "6px 14px",
    background: "none",
    border: "1px solid var(--border-color)",
    color: "var(--text-secondary)",
    borderRadius: "6px",
    cursor: "pointer",
  },
};

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  render() {
    if (this.state.error) {
      return (
        <div style={styles.container}>
          <h3 style={styles.heading}>Something went wrong</h3>
          <pre style={styles.message}>{this.state.error.message}</pre>
          <button
            className="hover-accent"
            style={styles.button}
            onClick={() => this.setState({ error: null })}
          >
            Try again
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
