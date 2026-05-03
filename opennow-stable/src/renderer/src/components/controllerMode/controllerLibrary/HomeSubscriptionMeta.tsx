import type { JSX } from "react";
import { Clock, Shield } from "lucide-react";
import type { SubscriptionInfo } from "@shared/gfn";
import { formatRemainingPlaytimeFromSubscription } from "../../../utils/usePlaytime";

function formatRenewalRelative(isoEnd: string | undefined): string | null {
  if (!isoEnd?.trim()) return null;
  const end = Date.parse(isoEnd);
  if (!Number.isFinite(end)) return null;
  const now = Date.now();
  const diffMs = end - now;
  const dayMs = 86_400_000;
  const days = Math.round(diffMs / dayMs);
  if (days < 0) return "Period ended";
  if (days === 0) return "Renews today";
  if (days === 1) return "Renews tomorrow";
  return `Renews in ${days} days`;
}

interface HomeSubscriptionMetaProps {
  subscriptionInfo: SubscriptionInfo | null;
}

export function HomeSubscriptionMeta({ subscriptionInfo }: HomeSubscriptionMetaProps): JSX.Element | null {
  if (!subscriptionInfo) return null;

  const tier = subscriptionInfo.membershipTier?.trim() || "Membership";
  const timeText = subscriptionInfo.isUnlimited
    ? "Unlimited"
    : `${formatRemainingPlaytimeFromSubscription(subscriptionInfo, 0)} left`;
  const renewal = formatRenewalRelative(subscriptionInfo.currentSpanEndDateTime);
  const state = subscriptionInfo.state?.trim();
  const warnState = state && state.toUpperCase() !== "ACTIVE";
  const blocked = subscriptionInfo.isGamePlayAllowed === false;

  return (
    <div className="xmb-ps5-focus-chips xmb-ps5-focus-chips--subscription" aria-label="Subscription">
      <span className="xmb-game-meta-chip xmb-game-meta-chip--tier">
        <Shield size={10} className="xmb-meta-icon" />
        {tier}
      </span>
      <span className="xmb-game-meta-chip xmb-game-meta-chip--playtime">
        <Clock size={10} className="xmb-meta-icon" />
        {timeText}
      </span>
      {renewal ? (
        <span className="xmb-game-meta-chip xmb-game-meta-chip--last-played">
          {renewal}
        </span>
      ) : null}
      {warnState || blocked ? (
        <span className="xmb-game-meta-chip xmb-game-meta-chip--sessions">
          {blocked ? "Play may be restricted" : `Status: ${state}`}
        </span>
      ) : null}
    </div>
  );
}
