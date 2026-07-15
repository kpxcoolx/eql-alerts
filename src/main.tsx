import { Component, StrictMode, type ErrorInfo, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./App.css";

type BoundaryState = { error: string | null };

class ErrorBoundary extends Component<{ children: ReactNode }, BoundaryState> {
  state: BoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): BoundaryState {
    return { error: error.message || String(error) };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("EQL Alerts UI crash", error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div
          style={{
            height: "100%",
            padding: 24,
            fontFamily: "system-ui, sans-serif",
            background: "#080a0d",
            color: "#eef2f7",
          }}
        >
          <h1 style={{ fontSize: 24, margin: "0 0 8px" }}>UI crashed</h1>
          <p style={{ color: "#8b95a8" }}>{this.state.error}</p>
          <button
            type="button"
            style={{ marginTop: 16, padding: "8px 12px" }}
            onClick={() => window.location.reload()}
          >
            Reload
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </StrictMode>
);
