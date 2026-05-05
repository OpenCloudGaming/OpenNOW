/// <reference types="vite/client" />

import type { OpenNowApi } from "@shared/gfn";

interface ImportMetaEnv {
  readonly VITE_OPENNOW_RUNTIME?: "electron" | "webos";
  readonly VITE_OPENNOW_VERSION?: string;
}

declare global {
  interface Window {
    openNow: OpenNowApi;
  }
}

export {};
