/// <reference types="vite/client" />

import type { OpenNowApi } from "@shared/gfn";

declare global {
  interface Window {
    openNow: OpenNowApi;
  }
}

export {};
