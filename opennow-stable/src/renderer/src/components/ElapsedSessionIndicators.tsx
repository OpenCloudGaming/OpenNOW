import { useEffect, useState } from "react";
import type { JSX } from "react";
import { Calendar, Clock3, Clock } from "lucide-react";

import type { SubscriptionInfo } from "@shared/gfn";

import { formatRemainingPlaytimeFromSubscription } from "../utils/usePlaytime";

function formatElapsed(totalSeconds: number): string {
  const safe = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(safe / 3600);
  const minutes = Math.floor((safe % 3600) / 60);
  const seconds = safe % 60;
  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, "0")}:${seconds.toString().padStart(2, "0")}`;
  }
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

interface SessionElapsedIndicatorProps {
  startedAtMs: number | null;
  active: boolean;
  className?: string;
  iconSize?: number;
}

export function SessionElapsedIndicator({ startedAtMs, active, className, iconSize = 14 }: SessionElapsedIndicatorProps): JSX.Element {
  const nowMs = useTicker(1000);
  const elapsedSeconds = active && startedAtMs != null ? Math.max(0, Math.floor((nowMs - startedAtMs) / 1000)) : 0;

  return (
    <span className={className}>
      <Clock3 size={iconSize} />
      <span>Session {formatElapsed(elapsedSeconds)}</span>
    </span>
  );
}


interface CurrentClockProps {
  className?: string;
}

function useTicker(tickMs: number): number {
  const [nowMs, setNowMs] = useState(() => Date.now());

  useEffect(() => {
    const timer = window.setInterval(() => {
      setNowMs(Date.now());
    }, tickMs);
    return () => window.clearInterval(timer);
  }, [tickMs]);

  return nowMs;
}

export function CurrentClock({ className }: CurrentClockProps): JSX.Element {
  const nowMs = useTicker(1000);
  return (
    <span className={className}>
      <Clock size={16} />
      <span>{new Date(nowMs).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</span>
    </span>
  );
}

interface RemainingPlaytimeIndicatorProps {
  subscriptionInfo: SubscriptionInfo | null;
  startedAtMs: number | null;
  active: boolean;
  className?: string;
}

export function RemainingPlaytimeIndicator({ subscriptionInfo, startedAtMs, active, className }: RemainingPlaytimeIndicatorProps): JSX.Element {
  const nowMs = useTicker(60_000);
  const elapsedSeconds = active && startedAtMs != null ? Math.max(0, Math.floor((nowMs - startedAtMs) / 1000)) : 0;
  const consumedHours = active ? Math.floor(elapsedSeconds / 60) / 60 : 0;
  const remainingPlaytimeText = formatRemainingPlaytimeFromSubscription(subscriptionInfo, consumedHours);

  return (
    <span className={className}>
      <Calendar size={10} />
      <span>{remainingPlaytimeText} left</span>
    </span>
  );
}
