import { useSessionRecoveryRuntime } from "./streamSession/useSessionRecoveryRuntime";
import { useRuntimeSnapshotPersistence } from "./streamSession/useRuntimeSnapshotPersistence";
import { useStreamRuntimeState } from "./streamSession/useStreamRuntimeState";

export function useStreamSession() {
  const runtime = useStreamRuntimeState();
  const recovery = useSessionRecoveryRuntime(runtime);
  const snapshot = useRuntimeSnapshotPersistence(runtime);
  return { runtime, recovery, snapshot };
}

export {
  ICE_DISCONNECTED_RECOVERY_GRACE_MS,
  RECOVERABLE_STREAM_STATUSES,
  SIGNALING_RECOVERY_POLL_INTERVAL_MS,
  SIGNALING_RECOVERY_STABLE_RESET_DELAY_MS,
  SIGNALING_RECOVERY_WINDOW_MS,
  SIGNALING_REMOTE_ICE_GRACE_MS,
  isRemoteSessionEndReason,
  nextSignalingRecoveryPollDelayMs,
  remoteSessionEndCode,
  readStreamClipboardText,
  sendStreamClipboardPaste,
  sleep,
  type SignalingRecoveryState,
} from "../lib/streamSessionHelpers";
