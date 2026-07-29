import { useCallback, useEffect, useRef } from "react";

export type ControllerFocusScroll = (resolve: () => HTMLElement | null | undefined) => void;

/**
 * Defers focus scrolling to the next frame so the DOM reflects the newly
 * focused card before it is scrolled into view. Only the most recent request
 * survives — rapid d-pad repeats collapse to a single scroll.
 */
export function useControllerFocusScroll(enabled: boolean): ControllerFocusScroll {
  const pendingFrameRef = useRef<number | null>(null);
  const enabledRef = useRef(enabled);
  enabledRef.current = enabled;

  const cancel = useCallback((): void => {
    if (pendingFrameRef.current !== null) {
      window.cancelAnimationFrame(pendingFrameRef.current);
      pendingFrameRef.current = null;
    }
  }, []);

  useEffect(() => {
    if (!enabled) cancel();
  }, [cancel, enabled]);

  useEffect(() => () => cancel(), [cancel]);

  return useCallback((resolve: () => HTMLElement | null | undefined): void => {
    if (!enabledRef.current) return;
    cancel();
    pendingFrameRef.current = window.requestAnimationFrame(() => {
      pendingFrameRef.current = null;
      if (!enabledRef.current) return;
      resolve()?.scrollIntoView({ inline: "nearest", block: "nearest", behavior: "auto" });
    });
  }, [cancel]);
}
