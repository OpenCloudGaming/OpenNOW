import type { StreamTimeWarning } from "../gfn/webrtcClient";

export type StreamStatus = "idle" | "queue" | "setup" | "starting" | "connecting" | "streaming";
export type StreamLoadingStatus = "queue" | "setup" | "starting" | "connecting";

export type StreamWarningState = {
  code: StreamTimeWarning["code"];
  message: string;
  tone: "warn" | "critical";
  secondsLeft?: number;
};

export type LocalSessionTimerWarningState = {
  stage: "free-tier-30m" | "free-tier-15m" | "free-tier-final-minute";
  shownAtMs: number;
};

export type LaunchErrorState = {
  stage: StreamLoadingStatus;
  title: string;
  description: string;
  codeLabel?: string;
};
