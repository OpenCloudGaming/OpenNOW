import { useSyncExternalStore } from "react";
import { backend, PluginState, Status } from "./backend";

export interface Snapshot {
  status: Status | null;
  state: PluginState | null;
  error: string | null;
}

let snapshot: Snapshot = { status: null, state: null, error: null };
const listeners = new Set<() => void>();

function publish(next: Partial<Snapshot>) {
  snapshot = { ...snapshot, ...next };
  listeners.forEach((listener) => listener());
}

export const store = {
  get: () => snapshot,
  set: publish,
  subscribe(listener: () => void) {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
};

export async function refresh() {
  try {
    const [status, state] = await Promise.all([backend.getStatus(), backend.getState()]);
    publish({ status, state, error: null });
  } catch (error) {
    publish({ error: String(error) });
  }
}

export function useSnapshot(): Snapshot {
  return useSyncExternalStore(store.subscribe, store.get);
}
