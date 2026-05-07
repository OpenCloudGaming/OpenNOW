import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Dispatch, RefObject, SetStateAction } from "react";

import type {
  AuthSession,
  SessionAdAction,
  SessionAdInfo,
  SessionAdReportRequest,
  SessionAdState,
  SessionInfo,
  SubscriptionInfo,
} from "@shared/gfn";
import {
  getPreferredSessionAdMediaUrl,
  getSessionAdDurationMs,
  getSessionAdItems,
  getSessionAdOpportunity,
  isSessionAdsRequired,
  isSessionQueuePaused,
} from "@shared/gfn";

import type { QueueAdPlaybackEvent, QueueAdPreviewHandle } from "../components/QueueAdPreview";
import {
  getActiveQueueAd,
  getEffectiveAdState,
  getNextAdReportAction,
  getNextQueueAd,
  mergePolledSessionState,
} from "../lib/queueAds";

type TranslateFunction = typeof import("../i18n").t;

const SESSION_AD_PROGRESS_CHECK_INTERVAL_MS = 1000;
const SESSION_AD_START_TIMEOUT_MS = 30000;
const SESSION_AD_FORCE_PLAY_TIMEOUT_MS = 10000;
const SESSION_AD_STUCK_TIMEOUT_MS = 30000;

type QueueAdCancelReason = "error" | "other";
type QueueAdErrorInfo = "Ad play timeout" | "Ad video is stuck" | "Error loading url";

type QueueAdMetrics = {
  startedAtMs: number | null;
  wasPausedAtLeastOnce: boolean;
};

type QueueAdPlaybackRuntime = {
  adId: string;
  phase: "playing" | "finishing";
};

type QueueAdReportOptions = {
  cancelReason?: QueueAdCancelReason;
  errorInfo?: QueueAdErrorInfo;
};

type QueueAdReportPayload = {
  watchedTimeInMs: number | undefined;
  pausedTimeInMs: number;
  cancelReason: QueueAdCancelReason | undefined;
  errorInfo: QueueAdErrorInfo | undefined;
};

export interface UseQueueAdRuntimeInput {
  authSession: AuthSession | null;
  effectiveStreamingBaseUrl: string;
  session: SessionInfo | null;
  sessionRef: RefObject<SessionInfo | null>;
  setQueuePosition: Dispatch<SetStateAction<number | undefined>>;
  setSession: Dispatch<SetStateAction<SessionInfo | null>>;
  subscriptionInfo: SubscriptionInfo | null;
  t: TranslateFunction;
}

export interface QueueAdRuntime {
  activeQueueAd: SessionAdInfo | undefined;
  activeQueueAdMediaUrl: string | undefined;
  effectiveAdState: SessionAdState | undefined;
  handleQueueAdPlaybackEvent: (event: QueueAdPlaybackEvent, adId: string) => void;
  queueAdPlaybackRef: RefObject<QueueAdPlaybackRuntime | null>;
  queueAdPreviewRef: RefObject<QueueAdPreviewHandle | null>;
}

export function useQueueAdRuntime({
  authSession,
  effectiveStreamingBaseUrl,
  session,
  sessionRef,
  setQueuePosition,
  setSession,
  subscriptionInfo,
  t,
}: UseQueueAdRuntimeInput): QueueAdRuntime {
  const [activeQueueAdId, setActiveQueueAdId] = useState<string | null>(null);
  const adReportQueueRef = useRef<Promise<void>>(Promise.resolve());
  const adReportStateRef = useRef<Record<string, SessionAdAction>>({});
  const adMetricsRef = useRef<Record<string, QueueAdMetrics>>({});
  const adMediaUrlCacheRef = useRef<Record<string, string>>({});
  const queueAdPlaybackRef = useRef<QueueAdPlaybackRuntime | null>(null);
  const queueAdPreviewRef = useRef<QueueAdPreviewHandle | null>(null);
  const activeQueueAdIdRef = useRef<string | null>(null);
  const adForcePlayTimeoutRef = useRef<number | null>(null);
  const adStartTimeoutRef = useRef<number | null>(null);
  const adProgressIntervalRef = useRef<number | null>(null);
  const adLastProgressTsRef = useRef<number | null>(null);
  const reportQueueAdActionRef = useRef<
    (adId: string, action: SessionAdAction, options?: QueueAdReportOptions) => void
  >(() => {});

  const clearQueueAdStartWatchdogs = useCallback((): void => {
    if (adForcePlayTimeoutRef.current) {
      clearTimeout(adForcePlayTimeoutRef.current);
      adForcePlayTimeoutRef.current = null;
    }
    if (adStartTimeoutRef.current) {
      clearTimeout(adStartTimeoutRef.current);
      adStartTimeoutRef.current = null;
    }
  }, []);

  const clearQueueAdProgressWatchdog = useCallback((): void => {
    if (adProgressIntervalRef.current) {
      clearInterval(adProgressIntervalRef.current);
      adProgressIntervalRef.current = null;
    }
    adLastProgressTsRef.current = null;
  }, []);

  const clearQueueAdWatchdogs = useCallback((): void => {
    clearQueueAdStartWatchdogs();
    clearQueueAdProgressWatchdog();
  }, [clearQueueAdProgressWatchdog, clearQueueAdStartWatchdogs]);

  const resetQueueAdMetrics = useCallback((adId: string): void => {
    adMetricsRef.current[adId] = {
      startedAtMs: null,
      wasPausedAtLeastOnce: false,
    };
  }, []);

  const markQueueAdStarted = useCallback((adId: string): void => {
    const current = adMetricsRef.current[adId];
    adMetricsRef.current[adId] = {
      startedAtMs: current?.startedAtMs ?? Date.now(),
      wasPausedAtLeastOnce: current?.wasPausedAtLeastOnce ?? false,
    };
  }, []);

  const markQueueAdPaused = useCallback((adId: string): void => {
    const current = adMetricsRef.current[adId];
    adMetricsRef.current[adId] = {
      startedAtMs: current?.startedAtMs ?? null,
      wasPausedAtLeastOnce: true,
    };
  }, []);

  const clearQueueAdMetrics = useCallback((adId: string): void => {
    delete adMetricsRef.current[adId];
  }, []);

  const getQueueAdReportPayload = useCallback((adId: string, action: SessionAdAction, options?: QueueAdReportOptions): QueueAdReportPayload => {
    const ad = getSessionAdItems(sessionRef.current?.adState).find((candidate) => candidate.adId === adId);
    const adLengthMs = getSessionAdDurationMs(ad);
    const snapshot = queueAdPreviewRef.current?.getSnapshot();
    const watchedTimeInMs =
      action === "finish" || action === "cancel"
        ? Math.max(0, Math.round((snapshot?.currentTime ?? 0) * 1000))
        : undefined;

    const metrics = adMetricsRef.current[adId];
    let pausedTimeInMs = 0;
    if (
      metrics?.startedAtMs &&
      metrics.wasPausedAtLeastOnce &&
      typeof adLengthMs === "number" &&
      Number.isFinite(adLengthMs) &&
      adLengthMs > 0
    ) {
      const elapsedMs = Date.now() - metrics.startedAtMs;
      if (elapsedMs > adLengthMs) {
        pausedTimeInMs = Math.round(elapsedMs - adLengthMs);
      }
    }

    return {
      watchedTimeInMs,
      pausedTimeInMs,
      cancelReason: action === "cancel" ? options?.cancelReason : undefined,
      errorInfo: action === "cancel" ? options?.errorInfo : undefined,
    };
  }, [sessionRef]);

  const armQueueAdStartWatchdogs = useCallback((adId: string): void => {
    clearQueueAdStartWatchdogs();
    adLastProgressTsRef.current = Date.now();

    adForcePlayTimeoutRef.current = window.setTimeout(() => {
      if (activeQueueAdIdRef.current !== adId) {
        return;
      }
      const lastAction = adReportStateRef.current[adId];
      if (lastAction === "start" || lastAction === "resume" || lastAction === "finish" || lastAction === "cancel") {
        return;
      }
      void queueAdPreviewRef.current?.attemptPlayback();
    }, SESSION_AD_FORCE_PLAY_TIMEOUT_MS);

    adStartTimeoutRef.current = window.setTimeout(() => {
      if (activeQueueAdIdRef.current !== adId) {
        return;
      }
      const lastAction = adReportStateRef.current[adId];
      if (lastAction === "start" || lastAction === "resume" || lastAction === "finish" || lastAction === "cancel") {
        return;
      }

      clearQueueAdWatchdogs();
      queueAdPlaybackRef.current = null;
      adReportStateRef.current[adId] = "cancel";
      reportQueueAdActionRef.current(adId, "cancel", {
        cancelReason: "error",
        errorInfo: "Ad play timeout",
      });
    }, SESSION_AD_START_TIMEOUT_MS);
  }, [clearQueueAdStartWatchdogs, clearQueueAdWatchdogs]);

  const ensureQueueAdProgressWatchdog = useCallback((adId: string): void => {
    adLastProgressTsRef.current = Date.now();
    if (adProgressIntervalRef.current) {
      return;
    }

    adProgressIntervalRef.current = window.setInterval(() => {
      if (activeQueueAdIdRef.current !== adId) {
        return;
      }

      const lastAction = adReportStateRef.current[adId];
      if (lastAction !== "start" && lastAction !== "resume") {
        return;
      }

      const snapshot = queueAdPreviewRef.current?.getSnapshot();
      if (!snapshot || snapshot.paused || snapshot.ended || isSessionQueuePaused(sessionRef.current?.adState)) {
        return;
      }

      const lastProgressTs = adLastProgressTsRef.current;
      if (!lastProgressTs || Date.now() - lastProgressTs < SESSION_AD_STUCK_TIMEOUT_MS) {
        return;
      }

      clearQueueAdWatchdogs();
      queueAdPlaybackRef.current = null;
      adReportStateRef.current[adId] = "cancel";
      reportQueueAdActionRef.current(adId, "cancel", {
        cancelReason: "error",
        errorInfo: "Ad video is stuck",
      });
    }, SESSION_AD_PROGRESS_CHECK_INTERVAL_MS);
  }, [clearQueueAdWatchdogs, sessionRef]);

  useEffect(() => {
    adReportStateRef.current = {};
    adMetricsRef.current = {};
    adReportQueueRef.current = Promise.resolve();
    adMediaUrlCacheRef.current = {};
    queueAdPlaybackRef.current = null;
    activeQueueAdIdRef.current = null;
    setActiveQueueAdId(null);
    clearQueueAdWatchdogs();
  }, [clearQueueAdWatchdogs, session?.sessionId]);

  const reportQueueAdAction = useCallback((adId: string, action: SessionAdAction, options?: QueueAdReportOptions): void => {
    const currentSession = sessionRef.current;
    if (!currentSession) {
      return;
    }

    const reportPayload = getQueueAdReportPayload(adId, action, options);

    const request: SessionAdReportRequest = {
      token: authSession?.tokens.idToken ?? authSession?.tokens.accessToken ?? undefined,
      streamingBaseUrl: currentSession.streamingBaseUrl ?? effectiveStreamingBaseUrl,
      serverIp: currentSession.serverIp,
      zone: currentSession.zone,
      sessionId: currentSession.sessionId,
      clientId: currentSession.clientId,
      deviceId: currentSession.deviceId,
      adId,
      action,
      ...reportPayload,
    };

    adReportQueueRef.current = adReportQueueRef.current
      .catch(() => undefined)
      .then(async () => {
        if (sessionRef.current?.sessionId !== request.sessionId) {
          return;
        }

        try {
          console.log(
            `[QueueAds] Sending ad update: action=${action}, adId=${adId}, sessionId=${request.sessionId}, zone=${request.zone}, ` +
              `serverIp=${request.serverIp}, queuePosition=${sessionRef.current?.queuePosition ?? "n/a"}, ` +
              `watchedTimeInMs=${request.watchedTimeInMs ?? "n/a"}, pausedTimeInMs=${request.pausedTimeInMs ?? 0}, ` +
              `cancelReason=${request.cancelReason ?? "n/a"}, errorInfo=${request.errorInfo ?? "n/a"}`,
          );
          const updated = await window.openNow.reportSessionAd(request);
          if (sessionRef.current?.sessionId !== updated.sessionId) {
            return;
          }

          console.log(
            `[QueueAds] Ad update succeeded: action=${action}, adId=${adId}, sessionId=${updated.sessionId}, ` +
              `status=${updated.status}, queuePosition=${updated.queuePosition ?? "n/a"}`,
          );
          console.log(
            `[QueueAds] Returned ad state: sessionId=${updated.sessionId}, adsRequired=${isSessionAdsRequired(updated.adState)}, ` +
              `queuePaused=${getSessionAdOpportunity(updated.adState)?.queuePaused ?? false}, gracePeriodSeconds=${getSessionAdOpportunity(updated.adState)?.gracePeriodSeconds ?? "n/a"}, ` +
              `adCount=${getSessionAdItems(updated.adState).length}`,
          );

          setSession((previous) => {
            if (!previous || previous.sessionId !== updated.sessionId) {
              return previous;
            }
            return mergePolledSessionState(previous, updated);
          });
          setQueuePosition(updated.queuePosition);
          const updatedAd = getSessionAdItems(updated.adState).find((candidate) => candidate.adId === adId);
          const updatedAdMediaUrl = getPreferredSessionAdMediaUrl(updatedAd);
          if (updatedAdMediaUrl) {
            adMediaUrlCacheRef.current[adId] = updatedAdMediaUrl;
          }

          if (action === "finish" || action === "cancel") {
            clearQueueAdMetrics(adId);
            if (queueAdPlaybackRef.current?.adId === adId) {
              queueAdPlaybackRef.current = null;
            }
          }
        } catch (error) {
          console.warn(`[QueueAds] Failed to report ${action} for ${adId}:`, error);

          if (action === "finish") {
            console.log(`[QueueAds] finish rejected — clearing local ad list for adId=${adId}`);
            delete adReportStateRef.current[adId];
            clearQueueAdMetrics(adId);
            if (queueAdPlaybackRef.current?.adId === adId) {
              queueAdPlaybackRef.current = null;
            }
            setSession((previous) => {
              if (!previous || !previous.adState) {
                return previous;
              }
              return {
                ...previous,
                adState: { ...previous.adState, sessionAds: [], ads: [], serverSentEmptyAds: false },
              };
            });
          }
        }
      });
  }, [authSession, clearQueueAdMetrics, effectiveStreamingBaseUrl, getQueueAdReportPayload, sessionRef, setQueuePosition, setSession]);

  const handleQueueAdPlaybackEvent = useCallback((event: QueueAdPlaybackEvent, adId: string): void => {
    const currentSession = sessionRef.current;
    const currentAd = getSessionAdItems(currentSession?.adState).find((ad) => ad.adId === adId);
    if (!currentAd) {
      return;
    }

    const lastAction = adReportStateRef.current[adId];

    if (event === "loadstart") {
      activeQueueAdIdRef.current = adId;
      setActiveQueueAdId((previous) => (previous === adId ? previous : adId));
      resetQueueAdMetrics(adId);
      queueAdPlaybackRef.current = null;
      clearQueueAdProgressWatchdog();
      armQueueAdStartWatchdogs(adId);
      return;
    }

    if (event === "timeupdate") {
      adLastProgressTsRef.current = Date.now();
      return;
    }

    if (event === "playing") {
      activeQueueAdIdRef.current = adId;
      setActiveQueueAdId((previous) => (previous === adId ? previous : adId));
      clearQueueAdStartWatchdogs();
      ensureQueueAdProgressWatchdog(adId);
      queueAdPlaybackRef.current = { adId, phase: "playing" };

      const nextAction = getNextAdReportAction(lastAction, "playing");
      if (!nextAction) {
        return;
      }
      if (nextAction === "start") {
        markQueueAdStarted(adId);
      }
      adReportStateRef.current[adId] = nextAction;
      reportQueueAdAction(adId, nextAction);
      return;
    }

    if (event === "paused") {
      if (queueAdPlaybackRef.current?.adId === adId) {
        queueAdPlaybackRef.current = null;
      }
      const nextAction = getNextAdReportAction(lastAction, "paused");
      if (nextAction) {
        markQueueAdPaused(adId);
        adReportStateRef.current[adId] = nextAction;
        reportQueueAdAction(adId, nextAction);
      }
      return;
    }

    if (event === "ended") {
      clearQueueAdWatchdogs();
      if (lastAction === "finish" || lastAction === "cancel") {
        return;
      }

      const nextAd = getNextQueueAd(currentSession?.adState, adId);
      if (nextAd) {
        activeQueueAdIdRef.current = nextAd.adId;
        setActiveQueueAdId((previous) => (previous === nextAd.adId ? previous : nextAd.adId));
      }

      queueAdPlaybackRef.current = { adId, phase: "finishing" };
      const nextAction = getNextAdReportAction(lastAction, "ended");
      if (!nextAction) {
        return;
      }
      adReportStateRef.current[adId] = nextAction;
      reportQueueAdAction(adId, nextAction);
      return;
    }

    if (event === "error") {
      clearQueueAdWatchdogs();
      if (lastAction === "finish" || lastAction === "cancel") {
        return;
      }
      queueAdPlaybackRef.current = null;
      adReportStateRef.current[adId] = "cancel";
      reportQueueAdAction(adId, "cancel", {
        cancelReason: "error",
        errorInfo: "Error loading url",
      });
    }
  }, [armQueueAdStartWatchdogs, clearQueueAdProgressWatchdog, clearQueueAdStartWatchdogs, clearQueueAdWatchdogs, ensureQueueAdProgressWatchdog, markQueueAdPaused, markQueueAdStarted, reportQueueAdAction, resetQueueAdMetrics, sessionRef]);

  useEffect(() => {
    reportQueueAdActionRef.current = reportQueueAdAction;
  }, [reportQueueAdAction]);

  const effectiveAdState = getEffectiveAdState(t, session, subscriptionInfo, authSession);
  const activeQueueAd = useMemo(
    () => getActiveQueueAd(effectiveAdState, activeQueueAdId),
    [activeQueueAdId, effectiveAdState],
  );
  const activeQueueAdMediaUrl = getPreferredSessionAdMediaUrl(activeQueueAd) ?? (activeQueueAd ? adMediaUrlCacheRef.current[activeQueueAd.adId] : undefined);

  useEffect(() => {
    const nextActiveAd = getActiveQueueAd(effectiveAdState, activeQueueAdIdRef.current);
    const nextActiveAdId = nextActiveAd?.adId ?? null;
    activeQueueAdIdRef.current = nextActiveAdId;
    setActiveQueueAdId((previous) => (previous === nextActiveAdId ? previous : nextActiveAdId));

    if (!nextActiveAdId) {
      clearQueueAdWatchdogs();
    }
  }, [clearQueueAdWatchdogs, effectiveAdState]);

  useEffect(() => {
    if (!activeQueueAd) {
      return;
    }

    const syncQueueAdVisibility = (): void => {
      const preview = queueAdPreviewRef.current;
      if (!preview) {
        return;
      }

      if (document.hidden || isSessionQueuePaused(effectiveAdState)) {
        preview.pause();
        return;
      }

      void preview.resume();
    };

    syncQueueAdVisibility();
    document.addEventListener("visibilitychange", syncQueueAdVisibility);
    return () => {
      document.removeEventListener("visibilitychange", syncQueueAdVisibility);
    };
  }, [activeQueueAd, effectiveAdState]);

  return {
    activeQueueAd,
    activeQueueAdMediaUrl,
    effectiveAdState,
    handleQueueAdPlaybackEvent,
    queueAdPlaybackRef,
    queueAdPreviewRef,
  };
}
