import { EventEmitter } from "node:events";

interface CacheRefreshErrorEvent {
  key: string;
  error: string;
}

class CacheEventBus extends EventEmitter {
  emit(event: "cache:refresh-start"): boolean;
  emit(event: "cache:refresh-success"): boolean;
  emit(event: "cache:refresh-error", details: CacheRefreshErrorEvent): boolean;
  emit(event: string, ...args: unknown[]): boolean {
    return super.emit(event, ...args);
  }
}

export const cacheEventBus = new CacheEventBus();
