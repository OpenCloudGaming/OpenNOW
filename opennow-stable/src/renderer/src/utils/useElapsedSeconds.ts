import { useEffect, useState } from "react";

function computeElapsedSeconds(startedAtMs: number | null): number {
  if (startedAtMs == null) return 0;
  return Math.max(0, Math.floor((Date.now() - startedAtMs) / 1000));
}

export function useElapsedSeconds(startedAtMs: number | null, active: boolean, tickMs = 1000): number {
  const [elapsedSeconds, setElapsedSeconds] = useState(() => computeElapsedSeconds(startedAtMs));

  useEffect(() => {
    if (!active || startedAtMs == null) {
      setElapsedSeconds(0);
      return;
    }

    let intervalId: number | null = null;
    let timeoutId: number | null = null;
    let cancelled = false;

    const update = () => {
      if (!cancelled) {
        setElapsedSeconds(computeElapsedSeconds(startedAtMs));
      }
    };

    update();

    const remainder = (Date.now() - startedAtMs) % tickMs;
    const initialDelay = remainder === 0 ? tickMs : tickMs - remainder;

    timeoutId = window.setTimeout(() => {
      update();
      intervalId = window.setInterval(update, tickMs);
    }, initialDelay);

    return () => {
      cancelled = true;
      if (timeoutId !== null) {
        window.clearTimeout(timeoutId);
      }
      if (intervalId !== null) {
        window.clearInterval(intervalId);
      }
    };
  }, [active, startedAtMs, tickMs]);

  return elapsedSeconds;
}
