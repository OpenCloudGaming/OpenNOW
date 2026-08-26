import { useRef, useState } from "react";
import type {
  ActiveSessionInfo,
  DirectLaunchRequest,
  GameInfo,
  NativeStreamerShortcutAction,
  PrintedWasteQueueData,
  SessionInfo,
} from "@shared/gfn";

import type {
  LaunchErrorState,
  LocalSessionTimerWarningState,
  StreamStatus,
  StreamWarningState,
} from "../../lib/appTypes";
import { loadRuntimeSnapshot, type RuntimeSnapshot } from "../../lib/runtimeSnapshot";
import type { GfnWebRtcClient } from "../../platforms/gfn/webrtcClient";
import type { SignalingRecoveryState } from "../../lib/streamSessionHelpers";
import type { StatsOverlayMode } from "../../utils/streamStatsHud";

export function useStreamRuntimeState() {
  const [session, setSession] = useState<SessionInfo | null>(null);
  const [streamStatus, setStreamStatus] = useState<StreamStatus>("idle");
  const [statsMode, setStatsMode] = useState<StatsOverlayMode>("off");
  const [antiAfkEnabled, setAntiAfkEnabled] = useState(false);
  const [antiAfkAckNonce, setAntiAfkAckNonce] = useState(0);
  const [nativeInputCaptureActive, setNativeInputCaptureActive] = useState(false);
  const [nativeInputBridgeReady, setNativeInputBridgeReady] = useState(false);
  const [streamingGame, setStreamingGame] = useState<GameInfo | null>(null);
  const [streamingStore, setStreamingStore] = useState<string | null>(null);
  const [queuePosition, setQueuePosition] = useState<number | undefined>();
  const [navbarActiveSession, setNavbarActiveSession] = useState<ActiveSessionInfo | null>(null);
  const [isResumingNavbarSession, setIsResumingNavbarSession] = useState(false);
  const [isTerminatingNavbarSession, setIsTerminatingNavbarSession] = useState(false);
  const [launchError, setLaunchError] = useState<LaunchErrorState | null>(null);
  const [pendingDirectLaunchRequest, setPendingDirectLaunchRequest] = useState<DirectLaunchRequest | null>(null);
  const [directLaunchConsoleMode, setDirectLaunchConsoleMode] = useState(false);
  const [queueModalGame, setQueueModalGame] = useState<GameInfo | null>(null);
  const [queueModalData, setQueueModalData] = useState<PrintedWasteQueueData | null>(null);
  const [sessionStartedAtMs, setSessionStartedAtMs] = useState<number | null>(null);
  const [remoteStreamWarning, setRemoteStreamWarning] = useState<StreamWarningState | null>(null);
  const [localSessionTimerWarning, setLocalSessionTimerWarning] = useState<LocalSessionTimerWarningState | null>(null);
  const [streamVolume, setStreamVolume] = useState(1);
  const [streamMicLevel, setStreamMicLevel] = useState(1);
  const [videoElementHasFrame, setVideoElementHasFrame] = useState(false);
  const [streamRevealPhase, setStreamRevealPhase] = useState<"covered" | "revealing" | "revealed">("covered");

  const videoRef = useRef<HTMLVideoElement | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const clientRef = useRef<GfnWebRtcClient | null>(null);
  const previousFreeTierRemainingSecondsRef = useRef<number | null>(null);
  const navbarSessionActionInFlightRef = useRef<"resume" | "terminate" | null>(null);
  const nativeStreamingRef = useRef(false);
  const handleStreamShortcutActionRef = useRef<((action: NativeStreamerShortcutAction) => void) | null>(null);
  const streamingGameRef = useRef<GameInfo | null>(null);
  const isStreamingRef = useRef(false);
  const sessionRef = useRef<SessionInfo | null>(null);
  const regionsRequestRef = useRef(0);
  const launchInFlightRef = useRef(false);
  const directLaunchAttemptIdRef = useRef<string | null>(null);
  const handledDirectLaunchIdsRef = useRef<Set<string>>(new Set());
  const runtimeSnapshotRef = useRef<RuntimeSnapshot | null>(loadRuntimeSnapshot());
  const claimResumePromisesRef = useRef<Map<string, Promise<void>>>(new Map());
  const launchAbortRef = useRef(false);
  const discordStreamingActivitySessionRef = useRef<string | null>(null);
  const streamStatusRef = useRef<StreamStatus>("idle");
  const nativeInputProtocolVersionRef = useRef<number | null>(null);
  const stableRecoveryResetTimerRef = useRef<number | null>(null);
  const remoteIceGraceTimerRef = useRef<number | null>(null);
  const remoteIceSeenForSessionRef = useRef<string | null>(null);
  const remoteIceRecoveryGenerationRef = useRef<number | null>(null);
  const awaitingRecoveryRemoteIceRef = useRef(false);
  const appUnloadingRef = useRef(false);
  const hasConfirmedRemoteIceRef = useRef(false);
  const latestIceConnectionStateRef = useRef<RTCIceConnectionState>("new");
  const iceDisconnectedRecoveryTimerRef = useRef<number | null>(null);
  const pendingControlledDisconnectsRef = useRef(0);
  const signalingRecoveryRef = useRef<SignalingRecoveryState>({
    attemptCount: 0,
    deadlineAtMs: null,
    inFlight: null,
    explicitShutdown: false,
    appId: null,
    generation: 0,
  });
  const directLaunchSessionSeenRef = useRef(false);

  return {
    session, setSession,
    streamStatus, setStreamStatus,
    statsMode, setStatsMode,
    antiAfkEnabled, setAntiAfkEnabled,
    antiAfkAckNonce, setAntiAfkAckNonce,
    nativeInputCaptureActive, setNativeInputCaptureActive,
    nativeInputBridgeReady, setNativeInputBridgeReady,
    streamingGame, setStreamingGame,
    streamingStore, setStreamingStore,
    queuePosition, setQueuePosition,
    navbarActiveSession, setNavbarActiveSession,
    isResumingNavbarSession, setIsResumingNavbarSession,
    isTerminatingNavbarSession, setIsTerminatingNavbarSession,
    launchError, setLaunchError,
    pendingDirectLaunchRequest, setPendingDirectLaunchRequest,
    directLaunchConsoleMode, setDirectLaunchConsoleMode,
    queueModalGame, setQueueModalGame,
    queueModalData, setQueueModalData,
    sessionStartedAtMs, setSessionStartedAtMs,
    remoteStreamWarning, setRemoteStreamWarning,
    localSessionTimerWarning, setLocalSessionTimerWarning,
    streamVolume, setStreamVolume,
    streamMicLevel, setStreamMicLevel,
    videoElementHasFrame, setVideoElementHasFrame,
    streamRevealPhase, setStreamRevealPhase,
    videoRef,
    audioRef,
    clientRef,
    previousFreeTierRemainingSecondsRef,
    navbarSessionActionInFlightRef,
    nativeStreamingRef,
    handleStreamShortcutActionRef,
    streamingGameRef,
    isStreamingRef,
    sessionRef,
    regionsRequestRef,
    launchInFlightRef,
    directLaunchAttemptIdRef,
    handledDirectLaunchIdsRef,
    runtimeSnapshotRef,
    claimResumePromisesRef,
    launchAbortRef,
    discordStreamingActivitySessionRef,
    streamStatusRef,
    nativeInputProtocolVersionRef,
    stableRecoveryResetTimerRef,
    remoteIceGraceTimerRef,
    remoteIceSeenForSessionRef,
    remoteIceRecoveryGenerationRef,
    awaitingRecoveryRemoteIceRef,
    appUnloadingRef,
    hasConfirmedRemoteIceRef,
    latestIceConnectionStateRef,
    iceDisconnectedRecoveryTimerRef,
    pendingControlledDisconnectsRef,
    signalingRecoveryRef,
    directLaunchSessionSeenRef,
  };
}

export type StreamRuntimeState = ReturnType<typeof useStreamRuntimeState>;
