import { useRef, useSyncExternalStore } from "react";

import type { StreamDiagnostics } from "../gfn/webrtcClient";

export interface StreamDiagnosticsStore {
  getSnapshot: () => StreamDiagnostics;
  getServerSnapshot: () => StreamDiagnostics;
  subscribe: (listener: () => void) => () => void;
  set: (value: StreamDiagnostics) => void;
}

export function createStreamDiagnosticsStore(initial: StreamDiagnostics): StreamDiagnosticsStore {
  let current = initial;
  const listeners = new Set<() => void>();

  const emit = () => {
    for (const listener of listeners) {
      listener();
    }
  };

  return {
    getSnapshot: () => current,
    getServerSnapshot: () => current,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    set: (value) => {
      if (Object.is(value, current)) {
        return;
      }
      current = value;
      emit();
    },
  };
}

export function useStreamDiagnosticsStore(store: StreamDiagnosticsStore): StreamDiagnostics {
  return useSyncExternalStore(store.subscribe, store.getSnapshot, store.getServerSnapshot);
}

export function useStreamDiagnosticsSelector<T>(
  store: StreamDiagnosticsStore,
  selector: (value: StreamDiagnostics) => T,
  isEqual: (prev: T, next: T) => boolean = Object.is,
): T {
  const selectionRef = useRef<T | null>(null);

  const getSelection = (snapshot: StreamDiagnostics): T => {
    const nextSelection = selector(snapshot);
    const previousSelection = selectionRef.current;
    if (previousSelection !== null && isEqual(previousSelection, nextSelection)) {
      return previousSelection;
    }
    selectionRef.current = nextSelection;
    return nextSelection;
  };

  return useSyncExternalStore(
    store.subscribe,
    () => getSelection(store.getSnapshot()),
    () => getSelection(store.getServerSnapshot()),
  );
}
