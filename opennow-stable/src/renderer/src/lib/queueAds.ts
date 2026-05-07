import type {
  AuthSession,
  SessionAdAction,
  SessionAdInfo,
  SessionAdState,
  SessionInfo,
  SubscriptionInfo,
} from "@shared/gfn";
import {
  getSessionAdItems,
  isSessionAdsRequired,
} from "@shared/gfn";

import { isSessionInQueue, isSessionReadyForConnect } from "./sessionState";

type TranslateFunction = typeof import("../i18n").t;

export function normalizeMembershipTier(tier: string | null | undefined): string | null {
  if (!tier) {
    return null;
  }
  const normalized = tier.trim().toUpperCase();
  return normalized.length > 0 ? normalized : null;
}

export function shouldShowQueueAdsForMembership(
  subscription: SubscriptionInfo | null,
  authSession: AuthSession | null,
): boolean {
  const effectiveTier = normalizeMembershipTier(subscription?.membershipTier ?? authSession?.user.membershipTier);
  return effectiveTier === null || effectiveTier === "FREE";
}

export function shouldUseQueueAdPolling(session: SessionInfo, subscription: SubscriptionInfo | null, authSession: AuthSession | null): boolean {
  return (
    shouldShowQueueAdsForMembership(subscription, authSession) &&
    isSessionInQueue(session) &&
    isSessionAdsRequired(session.adState)
  );
}

export function getEffectiveAdState(
  t: TranslateFunction,
  session: SessionInfo | null,
  subscription: SubscriptionInfo | null,
  authSession: AuthSession | null,
): SessionAdState | undefined {
  if (!session) {
    return undefined;
  }

  if (session.adState) {
    return session.adState;
  }

  if ((session.status === 1 || session.status === 2 || session.status === 3) && session.queuePosition !== undefined) {
    return {
      isAdsRequired: true,
      sessionAdsRequired: true,
      message: t("streamLoading.ads.freeTierQueueAdsBegin"),
      opportunity: {
        message: t("streamLoading.ads.freeTierQueueAdsBegin"),
      },
      sessionAds: [],
      ads: [],
      serverSentEmptyAds: true,
    };
  }

  if (!shouldShowQueueAdsForMembership(subscription, authSession)) {
    return undefined;
  }

  if (!isSessionInQueue(session)) {
    return undefined;
  }

  return {
    isAdsRequired: true,
    sessionAdsRequired: true,
    message: t("streamLoading.ads.freeTierQueueAdsBegin"),
    opportunity: {
      message: t("streamLoading.ads.freeTierQueueAdsBegin"),
    },
    sessionAds: [
      {
        adId: "queue-ad-placeholder",
        title: t("streamLoading.ads.advertisementInProgress"),
        description: t("streamLoading.ads.mediaWillAppear"),
      },
    ],
    ads: [
      {
        adId: "queue-ad-placeholder",
        title: t("streamLoading.ads.advertisementInProgress"),
        description: t("streamLoading.ads.mediaWillAppear"),
      },
    ],
  };
}

export function mergeAdState(
  previous: SessionAdState | undefined,
  next: SessionAdState | undefined,
): SessionAdState | undefined {
  if (!next) {
    return previous;
  }
  // Server only populates sessionAds in the first poll after session creation.
  // Later polls return sessionAdsRequired=true but sessionAds=null (serverSentEmptyAds=true),
  // which produces an empty ads array. Preserve the ad list from the most recent poll that
  // had URLs so the ad player can continue.
  // Do NOT restore when serverSentEmptyAds is false — that signals an explicit client-side
  // clear after a rejected finish action, and we must NOT bring the stale ad back.
  if (
    isSessionAdsRequired(next) &&
    next.serverSentEmptyAds === true &&
    getSessionAdItems(next).length === 0 &&
    previous?.sessionAds &&
    previous.sessionAds.length > 0
  ) {
    return { ...next, sessionAds: previous.sessionAds, ads: previous.ads };
  }
  return next;
}

export function mergePolledSessionState(previous: SessionInfo, next: SessionInfo): SessionInfo {
  if (isSessionReadyForConnect(next.status)) {
    return next;
  }

  return {
    ...next,
    adState: mergeAdState(previous.adState, next.adState),
    mediaConnectionInfo: next.mediaConnectionInfo ?? previous.mediaConnectionInfo,
  };
}

export function getNextAdReportAction(
  lastAction: SessionAdAction | undefined,
  playbackEvent: "playing" | "paused" | "ended",
): SessionAdAction | null {
  switch (playbackEvent) {
    case "playing":
      if (!lastAction) {
        return "start";
      }
      return lastAction === "pause" ? "resume" : null;
    case "paused":
      return lastAction === "start" || lastAction === "resume" ? "pause" : null;
    case "ended":
      return lastAction === "finish" || lastAction === "cancel" ? null : "finish";
    default:
      return null;
  }
}

export function getActiveQueueAd(
  adState: SessionAdState | undefined,
  activeAdId: string | null,
): SessionAdInfo | undefined {
  const ads = getSessionAdItems(adState);
  if (!ads.length) {
    return undefined;
  }

  if (activeAdId) {
    const matched = ads.find((ad) => ad.adId === activeAdId);
    if (matched) {
      return matched;
    }
  }

  return ads[0];
}

export function getNextQueueAd(
  adState: SessionAdState | undefined,
  activeAdId: string,
): SessionAdInfo | undefined {
  const ads = getSessionAdItems(adState);
  if (!ads.length) {
    return undefined;
  }

  const currentIndex = ads.findIndex((ad) => ad.adId === activeAdId);
  if (currentIndex < 0) {
    return ads[0];
  }

  return ads[currentIndex + 1];
}
