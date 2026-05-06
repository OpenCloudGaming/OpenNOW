import React from "react";
import ReactDOM from "react-dom/client";
import { scan } from "react-scan";

import { initLogCapture } from "@shared/logger";
import { App } from "./App";
import "./styles.css";

// Initialize log capture for renderer process
const logCapture = initLogCapture("renderer");

window.addEventListener("error", (event) => {
  logCapture.addEntry("error", "Window", event.message, [
    {
      source: event.filename,
      line: event.lineno,
      column: event.colno,
      error: event.error instanceof Error ? event.error.stack || event.error.message : event.error,
    },
  ]);
});

window.addEventListener("unhandledrejection", (event) => {
  logCapture.addEntry("error", "UnhandledPromise", "Unhandled promise rejection", [
    event.reason instanceof Error ? event.reason.stack || event.reason.message : event.reason,
  ]);
});

if (import.meta.env.DEV) {
  scan();
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
