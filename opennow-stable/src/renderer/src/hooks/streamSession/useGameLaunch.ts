import { useCallback } from "react";
import type {
  AuthSession,
  ExistingSessionStrategy,
  GameInfo,
  SessionInfo,
  Settings,
  SignalingConnectRequest,
  StreamSettings,
  SubscriptionInfo,
} from "@shared/gfn";
import { isSessionAdsRequired } from "@shared/gfn";
import { discordGameImageUrl } from "@shared/discord";

import {
  getOrRunCodecSupport,
  resolveSupportedStreamCodecs,
} from "../../lib/codecDiagnostics";
import { chooseAccountLinked, getEpicOwnershipLaunchError } from "../../lib/launchOwnership";
import {
  defaultVariantId,
  findSessionContextForAppId,
  getSelectedVariant,
  isNumericId,
} from "../../lib/gameCatalog";
import { mergePolledSessionState, shouldUseQueueAdPolling } from "../../lib/queueAds";
import {
  isSessionInQueue,
  isSessionReadyForConnect,
  toLaunchErrorState,
} from "../../lib/sessionState";
import { disposeSessionCreatedAfterAbort, sleep } from "../../lib/streamSessionHelpers";
import type { StreamLoadingStatus } from "../../lib/appTypes";
import type { StreamRuntimeState } from "./useStreamRuntimeState";

const SESSION_READY_POLL_INTERVAL_MS = 2000;
const SESSION_AD_POLL_INTERVAL_MS = 30000;

type TranslateFunction = typeof import("../../i18n").t;
type ResetLaunchRuntime = (options?: {
  keepLaunchError?: boolean;
  keepStreamingContext?: boolean;
}) => void;

export interface GameLaunchOptions {
  runtime: StreamRuntimeState;
  activeSessionProxyUrl?: string;
  allKnownGames: GameInfo[];
  authSession: AuthSession | null;
  buildCurrentStreamSettings: (
    subscription?: SubscriptionInfo | null,
    gameId?: string | null,
  ) => StreamSettings;
  buildSignalingConnectRequest: (session: SessionInfo) => SignalingConnectRequest;
  canLaunch: boolean;
  claimAndConnectSession: (session: import("@shared/gfn").ActiveSessionInfo) => Promise<void>;
  disconnectSignalingControlled: () => Promise<void>;
  effectiveStreamingBaseUrl: string;
  queueAdPlaybackRef: { current: unknown };
  refreshNavbarActiveSession: () => Promise<void>;
  resetLaunchRuntime: ResetLaunchRuntime;
  resetSignalingRecoveryState: () => void;
  resetStatsOverlayToPreference: () => void;
  resolveInstallToPlayStreamingBaseUrl: (
    game: GameInfo,
    subscription: SubscriptionInfo | null,
    token: string | undefined,
  ) => Promise<string | undefined>;
  resolveSubscriptionInfoForLaunch: () => Promise<SubscriptionInfo | null>;
  settings: Settings;
  startPlaytimeSession: (gameId: string) => void;
  stopSessionByTarget: (session: SessionInfo) => Promise<boolean>;
  subscriptionInfo: SubscriptionInfo | null;
  t: TranslateFunction;
  variantByGameId: Record<string, string>;
  warmNativeStreamerForLaunch: () => void;
}

export function useGameLaunch({
  runtime,
  activeSessionProxyUrl,
  allKnownGames,
  authSession,
  buildCurrentStreamSettings,
  buildSignalingConnectRequest,
  canLaunch,
  claimAndConnectSession,
  disconnectSignalingControlled,
  effectiveStreamingBaseUrl,
  queueAdPlaybackRef,
  refreshNavbarActiveSession,
  resetLaunchRuntime,
  resetSignalingRecoveryState,
  resetStatsOverlayToPreference,
  resolveInstallToPlayStreamingBaseUrl,
  resolveSubscriptionInfoForLaunch,
  settings,
  startPlaytimeSession,
  stopSessionByTarget,
  subscriptionInfo,
  t,
  variantByGameId,
  warmNativeStreamerForLaunch,
}: GameLaunchOptions) {
  const {
    clientRef,
    launchAbortRef,
    launchInFlightRef,
    navbarSessionActionInFlightRef,
    sessionRef,
    setLaunchError,
    setLocalSessionTimerWarning,
    setNavbarActiveSession,
    setQueuePosition,
    setRemoteStreamWarning,
    setSession,
    setSessionStartedAtMs,
    setStreamingGame,
    streamingGameRef,
    setStreamingStore,
    setStreamStatus,
    signalingRecoveryRef,
    streamStatus,
  } = runtime;

  // Play game handler
  const handlePlayGame = useCallback(async (game: GameInfo, options?: { bypassGuards?: boolean; streamingBaseUrl?: string; variantId?: string }) => {
    if (!canLaunch) return;

    console.log("handlePlayGame entry", {
      title: game.title,
      launchInFlight: launchInFlightRef.current,
      streamStatus,
      bypass: options?.bypassGuards ?? false,
    });

    if (!options?.bypassGuards && (launchInFlightRef.current || streamStatus !== "idle" || navbarSessionActionInFlightRef.current)) {
      console.warn("Ignoring play request: launch already in progress or stream not idle", {
        inFlight: launchInFlightRef.current,
        streamStatus,
        navbarSessionAction: navbarSessionActionInFlightRef.current,
      });
      return;
    }

    const selectedVariantId = options?.variantId ?? variantByGameId[game.id] ?? defaultVariantId(game);
    const selectedVariant = getSelectedVariant(game, selectedVariantId);
    const epicOwnershipError = getEpicOwnershipLaunchError(selectedVariant);
    if (epicOwnershipError) {
      setStreamingGame(game);
      setStreamingStore(selectedVariant?.store ?? null);
      setLaunchError({
        stage: "queue",
        title: epicOwnershipError.title,
        description: epicOwnershipError.description,
      });
      return;
    }

    launchInFlightRef.current = true;
    launchAbortRef.current = false;
    resetSignalingRecoveryState();
    let loadingStep: StreamLoadingStatus = "queue";
    const updateLoadingStep = (next: StreamLoadingStatus): void => {
      loadingStep = next;
      setStreamStatus(next);
    };

    setSessionStartedAtMs(null);
    setRemoteStreamWarning(null);
    setLocalSessionTimerWarning(null);
    setLaunchError(null);
    resetStatsOverlayToPreference();
    startPlaytimeSession(game.id);
    updateLoadingStep("queue");
    setQueuePosition(undefined);
    warmNativeStreamerForLaunch();
    let launchGameContext: GameInfo = game;

    try {
      const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;

      // Resolve appId
      let appId: string | null = null;
      if (isNumericId(selectedVariantId)) {
        appId = selectedVariantId;
      } else if (isNumericId(game.launchAppId)) {
        appId = game.launchAppId;
      }

      if (!appId && token) {
        try {
          const resolved = await window.openNow.resolveLaunchAppId({
            token,
            providerStreamingBaseUrl: effectiveStreamingBaseUrl,
            proxyUrl: activeSessionProxyUrl,
            appIdOrUuid: game.uuid ?? selectedVariantId,
          });
          if (resolved && isNumericId(resolved)) {
            appId = resolved;
          }
        } catch {
          // Ignore resolution errors
        }
      }

      if (launchAbortRef.current) return;

      if (!appId) {
        throw new Error("Could not resolve numeric appId for this game");
      }

      const numericAppId = Number(appId);
      signalingRecoveryRef.current.appId = numericAppId;
      const matchedGameContext = findSessionContextForAppId(allKnownGames, variantByGameId, numericAppId) ?? {
        game,
        variant: selectedVariant,
      };
      const launchVariant = matchedGameContext.variant ?? selectedVariant;
      launchGameContext = matchedGameContext.game;
      streamingGameRef.current = matchedGameContext.game;
      setStreamingGame(matchedGameContext.game);
      setStreamingStore(launchVariant?.store ?? null);

      const launchSubscription = await resolveSubscriptionInfoForLaunch();
      if (launchAbortRef.current) return;
      const streamSettings = buildCurrentStreamSettings(launchSubscription, matchedGameContext.game.id);
      const i2pStorageRegionBaseUrl = await resolveInstallToPlayStreamingBaseUrl(
        matchedGameContext.game,
        launchSubscription,
        token || undefined,
      );
      if (launchAbortRef.current) return;
      const launchStreamingBaseUrl = i2pStorageRegionBaseUrl ?? options?.streamingBaseUrl ?? effectiveStreamingBaseUrl;
      let existingSessionStrategy: ExistingSessionStrategy | undefined;
      let activeSessionGpuType: string | undefined;

      // Check for active sessions first
      if (token) {
        try {
          const activeSessions = await window.openNow.getActiveSessions(token, launchStreamingBaseUrl);
          if (launchAbortRef.current) return;
          if (activeSessions.length > 0) {
            activeSessionGpuType = activeSessions.find(
              (entry) => entry.appId === numericAppId && entry.gpuType?.trim(),
            )?.gpuType ?? activeSessions.find((entry) => entry.gpuType?.trim())?.gpuType;
            // Only claim sessions that are already paused/ready (status 2 or 3).
            // Status=1 sessions are still in queue/setup; sending a RESUME claim
            // skips the queue/ad phase entirely. Let them fall through to
            // createSession so the polling loop handles queue position and ads.
            const matchingSession = activeSessions.find((entry) => entry.appId === numericAppId && (entry.status === 2 || entry.status === 3)) ?? null;
            const otherSession = activeSessions.find((s) => s.status === 2 || s.status === 3) ?? null;

            if (matchingSession) {
              await claimAndConnectSession(matchingSession);
              setNavbarActiveSession(null);
              return;
            }

            if (otherSession) {
              const choice = await window.openNow.showSessionConflictDialog();
              if (launchAbortRef.current) return;
              if (choice === "cancel") {
                resetLaunchRuntime();
                return;
              }
              if (choice === "resume") {
                await claimAndConnectSession(otherSession);
                setNavbarActiveSession(null);
                return;
              }
              if (choice === "new") {
                existingSessionStrategy = "force-new";
              }
            }
          }
        } catch (error) {
          console.error("Failed to claim/resume session:", error);
          // Continue to create new session
        }
      }

      const sessionProxyUrl = activeSessionProxyUrl;
      const supportedCodecs = resolveSupportedStreamCodecs(await getOrRunCodecSupport());
      if (launchAbortRef.current) return;

      // Create new session
      let newSession = await window.openNow.createSession({
        token: token || undefined,
        streamingBaseUrl: launchStreamingBaseUrl,
        appId,
        internalTitle: game.title,
        discordGameImageUrl: discordGameImageUrl(game),
        accountLinked: chooseAccountLinked(game, selectedVariant),
        enablePersistingInGameSettings: settings.enablePersistingInGameSettings,
        supportsInGameSettingsPersistence: launchVariant?.supportsInGameSettingsPersistence === true,
        existingSessionStrategy,
        proxyUrl: sessionProxyUrl,
        zone: "prod",
        settings: streamSettings,
        supportedCodecs,
      });

      if (!newSession.gpuType?.trim() && activeSessionGpuType) {
        newSession = { ...newSession, gpuType: activeSessionGpuType };
      }

      if (await disposeSessionCreatedAfterAbort(
        launchAbortRef.current,
        newSession,
        stopSessionByTarget,
      )) {
        return;
      }

      setSession(newSession);
      setQueuePosition(newSession.queuePosition);

      // Poll for readiness.
      // Queue and setup/starting modes wait indefinitely until the session becomes ready
      // or the launch is explicitly aborted. Some rigs take much longer than 180s.
      let finalSession: SessionInfo | null = null;
      let latestSession = newSession;
      let isInQueueMode = isSessionInQueue(newSession);
      let attempt = 0;

      while (true) {
        attempt++;

        const pollIntervalMs = shouldUseQueueAdPolling(latestSession, subscriptionInfo, authSession)
          ? SESSION_AD_POLL_INTERVAL_MS
          : SESSION_READY_POLL_INTERVAL_MS;

        // Sleep in small ticks during ad-polling intervals so the loop can react
        // quickly when reportSessionAd clears isAdsRequired (which only updates
        // sessionRef, not the local latestSession variable).  Standard 2 s intervals
        // are kept as a single sleep since they're already short.
        if (pollIntervalMs > SESSION_READY_POLL_INTERVAL_MS) {
          const tickMs = 500;
          let elapsed = 0;
          while (elapsed < pollIntervalMs) {
            await sleep(tickMs);
            elapsed += tickMs;
            if (launchAbortRef.current) return;
            // Sync ad-action responses from sessionRef into the local tracking variable
            // so shouldUseQueueAdPolling sees the updated adState immediately.
            const refSession = sessionRef.current;
            if (refSession && refSession.sessionId === latestSession.sessionId) {
              latestSession = mergePolledSessionState(latestSession, refSession);
            }
            // Break out of the sleep early when ads are no longer required.
            if (!shouldUseQueueAdPolling(latestSession, subscriptionInfo, authSession)) {
              break;
            }
          }
        } else {
          await sleep(pollIntervalMs);
        }

        if (shouldUseQueueAdPolling(latestSession, subscriptionInfo, authSession) && queueAdPlaybackRef.current) {
          const graceDeadline = Date.now() + 5000;
          while (queueAdPlaybackRef.current && Date.now() < graceDeadline) {
            await sleep(200);
            if (launchAbortRef.current) {
              return;
            }
          }
        }

        if (launchAbortRef.current) {
          return;
        }

        if (launchAbortRef.current) {
          return;
        }

        const polled = await window.openNow.pollSession({
          token: token || undefined,
          streamingBaseUrl: newSession.streamingBaseUrl ?? effectiveStreamingBaseUrl,
          serverIp: newSession.serverIp,
          zone: newSession.zone,
          sessionId: newSession.sessionId,
          clientId: newSession.clientId,
          deviceId: newSession.deviceId,
          proxyUrl: sessionProxyUrl,
        });

        if (launchAbortRef.current) {
          return;
        }

        const mergedSession = mergePolledSessionState(latestSession, polled);
        latestSession = mergedSession;

        setSession(mergedSession);
        setQueuePosition(mergedSession.queuePosition);

        // Check if queue just cleared so the loading UI can transition to setup mode.
        isInQueueMode = isSessionInQueue(mergedSession);

        console.log(
          `Poll attempt ${attempt}: status=${mergedSession.status}, seatSetupStep=${mergedSession.seatSetupStep ?? "n/a"}, queuePosition=${mergedSession.queuePosition ?? "n/a"}, serverIp=${mergedSession.serverIp}, queueMode=${isInQueueMode}, adsRequired=${isSessionAdsRequired(mergedSession.adState)}`,
        );

        if (isSessionReadyForConnect(mergedSession.status)) {
          finalSession = mergedSession;
          break;
        }

        // Update status based on session state
        if (isInQueueMode) {
          updateLoadingStep("queue");
        } else if (mergedSession.status === 1) {
          updateLoadingStep("setup");
        }

      }

      // finalSession is guaranteed to be set here (we only exit the loop via break when session is ready)

      setQueuePosition(undefined);
      updateLoadingStep("connecting");

      // Use finalSession (the status=2 poll result) as the authoritative source for
      // signaling coordinates — it carries the real server IP resolved at the moment
      // the rig became ready. sessionRef.current may still hold stale zone-LB data
      // from a prior React render cycle.
      const sessionToConnect = finalSession ?? sessionRef.current ?? newSession;
      console.log("Connecting signaling with:", {
        sessionId: sessionToConnect.sessionId,
        signalingServer: sessionToConnect.signalingServer,
        signalingUrl: sessionToConnect.signalingUrl,
        status: sessionToConnect.status,
      });

      await window.openNow.connectSignaling(buildSignalingConnectRequest(sessionToConnect));
    } catch (error) {
      if (launchAbortRef.current) {
        return;
      }
      console.error("Launch failed:", error);
      setLaunchError(toLaunchErrorState(t, error, loadingStep, launchGameContext));
      await disconnectSignalingControlled();
      clientRef.current?.dispose();
      clientRef.current = null;
      resetLaunchRuntime({ keepLaunchError: true, keepStreamingContext: true });
      void refreshNavbarActiveSession();
    } finally {
      launchInFlightRef.current = false;
    }
  }, [
    authSession,
    activeSessionProxyUrl,
    allKnownGames,
    buildCurrentStreamSettings,
    buildSignalingConnectRequest,
    claimAndConnectSession,
    effectiveStreamingBaseUrl,
    refreshNavbarActiveSession,
    resetSignalingRecoveryState,
    resetLaunchRuntime,
    resetStatsOverlayToPreference,
    resolveInstallToPlayStreamingBaseUrl,
    resolveSubscriptionInfoForLaunch,
    canLaunch,
    settings.enablePersistingInGameSettings,
    stopSessionByTarget,
    streamStatus,
    t,
    variantByGameId,
    warmNativeStreamerForLaunch,
  ]);

  return { handlePlayGame };
}
