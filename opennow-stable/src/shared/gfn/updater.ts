export type AppUpdaterStatus =
  | "disabled"
  | "idle"
  | "checking"
  | "available"
  | "not-available"
  | "downloading"
  | "downloaded"
  | "error";

export interface AppUpdaterProgress {
  percent: number;
  transferred: number;
  total: number;
  bytesPerSecond: number;
}

export interface AppUpdaterState {
  status: AppUpdaterStatus;
  currentVersion: string;
  availableVersion?: string;
  downloadedVersion?: string;
  progress?: AppUpdaterProgress;
  lastCheckedAt?: number;
  message?: string;
  errorCode?: string;
  updateSource: "github-releases";
  canCheck: boolean;
  canDownload: boolean;
  canInstall: boolean;
  isPackaged: boolean;
}
