import React from "react";
import ReactDOM from "react-dom/client";

import { initLogCapture } from "@shared/logger";
import { App } from "./App";
import "./styles.css";

// Initialize log capture for renderer process
initLogCapture("renderer");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
