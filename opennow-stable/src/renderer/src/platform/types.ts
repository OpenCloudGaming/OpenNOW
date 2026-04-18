import type { OpenNowApi } from "@shared/gfn";

export interface PlatformCapabilities {
  isAndroid: boolean;
  isElectron: boolean;
  remoteIceCandidatesIncludeUsernameFragment: boolean;
  supportsQuitApp: boolean;
  supportsPointerLockToggle: boolean;
  supportsDesktopFullscreen: boolean;
  supportsLogExport: boolean;
  supportsCacheDeletion: boolean;
  supportsMediaFolderAccess: boolean;
  supportsScreenshotExport: boolean;
  supportsPersistentMedia: boolean;
  supportsKeyboardShortcuts: boolean;
  supportsControllerExitApp: boolean;
}

export interface PlatformInfo {
  kind: "electron" | "android";
  capabilities: PlatformCapabilities;
}

export interface OpenNowPlatform {
  info: PlatformInfo;
  api: OpenNowApi;
}
