import React from "react";
import ReactDOM from "react-dom/client";
import { scan } from "react-scan";

import { initLogCapture } from "@shared/logger";
import { App } from "./App";
import "./styles.css";

// Initialize log capture for renderer process
initLogCapture("renderer");

if (import.meta.env.DEV) {
  scan();
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
