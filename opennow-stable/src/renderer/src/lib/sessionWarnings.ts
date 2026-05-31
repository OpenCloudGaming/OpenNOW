import type { SubscriptionInfo } from "@shared/gfn";

import type { LocalSessionTimerWarningState, StreamWarningState } from "./appTypes";
import { normalizeMembershipTier } from "./queueAds";

type TranslateFunction = typeof import("../i18n").t;

export function warningTone(code: StreamWarningState["code"]): StreamWarningState["tone"] {
  if (code === 3) {
    return "critical";
  }
  return "warn";
}

export function warningMessage(t: TranslateFunction, code: StreamWarningState["code"]): string {
  if (code === 1) return t("session.warnings.sessionTimeLimitApproaching");
  if (code === 2) return t("session.warnings.idleTimeoutApproaching");
  return t("session.warnings.maximumSessionTimeApproaching");
}

export function shouldShowFreeTierSessionWarnings(subscription: SubscriptionInfo | null): boolean {
  return normalizeMembershipTier(subscription?.membershipTier) === "FREE";
}

export function getSessionLimitSecondsForTier(tier: string | null | undefined): number | null {
  switch (normalizeMembershipTier(tier)) {
    case "FREE":
      return 60 * 60;
    case "PRIORITY":
    case "PERFORMANCE":
      return 6 * 60 * 60;
    case "ULTIMATE":
      return 8 * 60 * 60;
    default:
      return null;
  }
}

export function hasCrossedWarningThreshold(
  previousSeconds: number | null,
  currentSeconds: number,
  thresholdSeconds: number,
): boolean {
  if (previousSeconds === null) {
    return currentSeconds === thresholdSeconds;
  }
  return previousSeconds > thresholdSeconds && currentSeconds <= thresholdSeconds;
}

export function getLocalSessionTimerWarning(
  t: TranslateFunction,
  stage: LocalSessionTimerWarningState["stage"],
  secondsLeft: number,
): StreamWarningState {
  if (stage === "free-tier-30m") {
    return {
      code: 1,
      message: t("session.warnings.freeTier30MinutesRemaining"),
      tone: "warn",
    };
  }

  if (stage === "free-tier-15m") {
    return {
      code: 1,
      message: t("session.warnings.freeTier15MinutesRemaining"),
      tone: "warn",
    };
  }

  return {
    code: 1,
    message: t("session.warnings.freeTierEndsSoon"),
    tone: "critical",
    secondsLeft: Math.max(0, secondsLeft),
  };
}
