import { useCallback } from "react";

import { SIGNALING_RECOVERY_STABLE_RESET_DELAY_MS } from "../../lib/streamSessionHelpers";
import type { StreamRuntimeState } from "./useStreamRuntimeState";

type RecoveryRuntime = Pick<
  StreamRuntimeState,
  | "awaitingRecoveryRemoteIceRef"
  | "hasConfirmedRemoteIceRef"
  | "iceDisconnectedRecoveryTimerRef"
  | "latestIceConnectionStateRef"
  | "pendingControlledDisconnectsRef"
  | "remoteIceGraceTimerRef"
  | "remoteIceRecoveryGenerationRef"
  | "remoteIceSeenForSessionRef"
  | "sessionRef"
  | "signalingRecoveryRef"
  | "stableRecoveryResetTimerRef"
  | "streamStatusRef"
>;

export function useSessionRecoveryRuntime(runtime: RecoveryRuntime) {
  const {
    awaitingRecoveryRemoteIceRef,
    hasConfirmedRemoteIceRef,
    iceDisconnectedRecoveryTimerRef,
    latestIceConnectionStateRef,
    pendingControlledDisconnectsRef,
    remoteIceGraceTimerRef,
    remoteIceRecoveryGenerationRef,
    remoteIceSeenForSessionRef,
    sessionRef,
    signalingRecoveryRef,
    stableRecoveryResetTimerRef,
    streamStatusRef,
  } = runtime;

  const clearRecoveryTimers = useCallback((): void => {
    for (const timerRef of [
      stableRecoveryResetTimerRef,
      remoteIceGraceTimerRef,
      iceDisconnectedRecoveryTimerRef,
    ]) {
      if (timerRef.current !== null) {
        window.clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    }
  }, [iceDisconnectedRecoveryTimerRef, remoteIceGraceTimerRef, stableRecoveryResetTimerRef]);

  const resetRecoveryConnectionState = useCallback((): void => {
    clearRecoveryTimers();
    remoteIceSeenForSessionRef.current = null;
    remoteIceRecoveryGenerationRef.current = null;
    awaitingRecoveryRemoteIceRef.current = false;
    hasConfirmedRemoteIceRef.current = false;
    latestIceConnectionStateRef.current = "new";
    pendingControlledDisconnectsRef.current = 0;
  }, [
    awaitingRecoveryRemoteIceRef,
    clearRecoveryTimers,
    hasConfirmedRemoteIceRef,
    latestIceConnectionStateRef,
    pendingControlledDisconnectsRef,
    remoteIceRecoveryGenerationRef,
    remoteIceSeenForSessionRef,
  ]);

  const resetSignalingRecoveryState = useCallback((options?: {
    keepExplicitShutdown?: boolean;
  }): void => {
    resetRecoveryConnectionState();
    signalingRecoveryRef.current.generation += 1;
    signalingRecoveryRef.current.attemptCount = 0;
    signalingRecoveryRef.current.deadlineAtMs = null;
    signalingRecoveryRef.current.inFlight = null;
    signalingRecoveryRef.current.appId = null;
    if (!options?.keepExplicitShutdown) {
      signalingRecoveryRef.current.explicitShutdown = false;
    }
  }, [resetRecoveryConnectionState, signalingRecoveryRef]);

  const markExplicitSignalingShutdown = useCallback((): void => {
    resetRecoveryConnectionState();
    signalingRecoveryRef.current.generation += 1;
    signalingRecoveryRef.current.explicitShutdown = true;
    signalingRecoveryRef.current.deadlineAtMs = null;
    signalingRecoveryRef.current.inFlight = null;
  }, [resetRecoveryConnectionState, signalingRecoveryRef]);

  const isRecoveryGenerationCurrent = useCallback((generation: number): boolean => {
    const state = signalingRecoveryRef.current;
    return state.generation === generation && !state.explicitShutdown;
  }, [signalingRecoveryRef]);

  const scheduleStableRecoveryReset = useCallback((sessionId: string): void => {
    if (stableRecoveryResetTimerRef.current !== null) {
      window.clearTimeout(stableRecoveryResetTimerRef.current);
    }
    stableRecoveryResetTimerRef.current = window.setTimeout(() => {
      stableRecoveryResetTimerRef.current = null;
      if (
        streamStatusRef.current !== "streaming"
        || sessionRef.current?.sessionId !== sessionId
      ) {
        return;
      }
      console.log(
        `[Recovery] Stream remained stable for ${SIGNALING_RECOVERY_STABLE_RESET_DELAY_MS}ms; resetting recovery budget`,
      );
      resetSignalingRecoveryState({ keepExplicitShutdown: true });
    }, SIGNALING_RECOVERY_STABLE_RESET_DELAY_MS);
  }, [resetSignalingRecoveryState, sessionRef, stableRecoveryResetTimerRef, streamStatusRef]);

  const disconnectSignalingControlled = useCallback(async (): Promise<void> => {
    pendingControlledDisconnectsRef.current += 1;
    await window.openNow.disconnectSignaling().catch(() => {});
  }, [pendingControlledDisconnectsRef]);

  return {
    clearRecoveryTimers,
    disconnectSignalingControlled,
    isRecoveryGenerationCurrent,
    markExplicitSignalingShutdown,
    resetRecoveryConnectionState,
    resetSignalingRecoveryState,
    scheduleStableRecoveryReset,
  };
}
