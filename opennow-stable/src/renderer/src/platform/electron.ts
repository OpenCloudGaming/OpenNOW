import type { OpenNowApi } from "@shared/gfn";

import type { OpenNowPlatform } from "./types";

const api = window.openNow as OpenNowApi;

export const electronPlatform: OpenNowPlatform = {
  info: {
    kind: "electron",
    capabilities: {
      isAndroid: false,
      isElectron: true,
      supportsQuitApp: true,
      supportsPointerLockToggle: true,
      supportsDesktopFullscreen: true,
      supportsLogExport: true,
      supportsCacheDeletion: true,
      supportsMediaFolderAccess: true,
      supportsScreenshotExport: true,
      supportsPersistentMedia: true,
      supportsKeyboardShortcuts: true,
      supportsControllerExitApp: true,
    },
  },
  api,
};
