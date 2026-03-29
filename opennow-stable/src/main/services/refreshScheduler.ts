import type { GameInfo } from "@shared/gfn";
import { cacheEventBus } from "./cacheEventBus";

export interface RefreshAuthContext {
  token: string;
  providerStreamingBaseUrl?: string;
}

type FetchFunction<T> = (token: string, providerStreamingBaseUrl?: string) => Promise<T>;
type PublicFetchFunction = () => Promise<GameInfo[]>;

class RefreshScheduler {
  private refreshTimer: ReturnType<typeof setInterval> | null = null;
  private isRefreshing: boolean = false;
  private authContext: RefreshAuthContext | null = null;
  private fetchMainGames: FetchFunction<GameInfo[]> | null = null;
  private fetchLibraryGames: FetchFunction<GameInfo[]> | null = null;
  private fetchPublicGames: PublicFetchFunction | null = null;
  private refreshIntervalMs: number = 12 * 60 * 60 * 1000;

  initialize(
    fetchMainGames: FetchFunction<GameInfo[]>,
    fetchLibraryGames: FetchFunction<GameInfo[]>,
    fetchPublicGames: PublicFetchFunction,
  ): void {
    this.fetchMainGames = fetchMainGames;
    this.fetchLibraryGames = fetchLibraryGames;
    this.fetchPublicGames = fetchPublicGames;
    console.log(`[CACHE] RefreshScheduler initialized (interval: ${this.refreshIntervalMs / 60000} minutes)`);
  }

  updateAuthContext(token: string, providerStreamingBaseUrl?: string): void {
    this.authContext = { token, providerStreamingBaseUrl };
    console.log(`[CACHE] Auth context updated for refresh scheduler`);
  }

  start(): void {
    if (this.refreshTimer) {
      console.warn(`[CACHE] RefreshScheduler already started`);
      return;
    }

    if (!this.fetchMainGames || !this.fetchLibraryGames || !this.fetchPublicGames) {
      console.error(`[CACHE] Cannot start RefreshScheduler: fetch functions not initialized`);
      return;
    }

    console.log(`[CACHE] Starting RefreshScheduler`);
    this.performRefresh();
    this.refreshTimer = setInterval(() => {
      void this.performRefresh();
    }, this.refreshIntervalMs);
  }

  stop(): void {
    if (!this.refreshTimer) {
      console.log(`[CACHE] RefreshScheduler already stopped`);
      return;
    }

    clearInterval(this.refreshTimer);
    this.refreshTimer = null;
    console.log(`[CACHE] RefreshScheduler stopped`);
  }

  async performRefresh(): Promise<void> {
    if (this.isRefreshing) {
      console.log(`[CACHE] Refresh already in progress, skipping`);
      return;
    }

    if (!this.authContext) {
      console.log(`[CACHE] Auth context not available, skipping refresh`);
      return;
    }

    if (!this.fetchMainGames || !this.fetchLibraryGames || !this.fetchPublicGames) {
      console.error(`[CACHE] Fetch functions not available`);
      return;
    }

    this.isRefreshing = true;
    const startTime = Date.now();
    console.log(`[CACHE] Refresh cycle started`);

    try {
      cacheEventBus.emit("cache:refresh-start");

      const results = await Promise.allSettled([
        this.fetchMainGames(this.authContext.token, this.authContext.providerStreamingBaseUrl),
        this.fetchLibraryGames(this.authContext.token, this.authContext.providerStreamingBaseUrl),
        this.fetchPublicGames(),
      ]);

      let hasErrors = false;
      for (let i = 0; i < results.length; i++) {
        const result = results[i];
        const name = ["main", "library", "public"][i];

        if (result.status === "rejected") {
          hasErrors = true;
          console.error(`[CACHE] Refresh failed for ${name} games:`, result.reason);
          cacheEventBus.emit("cache:refresh-error", {
            key: `games:${name}`,
            error: result.reason instanceof Error ? result.reason.message : String(result.reason),
          });
        }
      }

      const duration = Date.now() - startTime;
      console.log(`[CACHE] Refresh cycle completed in ${duration}ms`);

      if (!hasErrors) {
        cacheEventBus.emit("cache:refresh-success");
      }
    } catch (error) {
      console.error(`[CACHE] Refresh cycle error:`, error);
      cacheEventBus.emit("cache:refresh-error", {
        key: "refresh-cycle",
        error: error instanceof Error ? error.message : "Unknown error",
      });
    } finally {
      this.isRefreshing = false;
    }
  }

  async manualRefresh(): Promise<void> {
    console.log(`[CACHE] Manual refresh requested`);
    await this.performRefresh();
  }

  setRefreshInterval(intervalMs: number): void {
    console.log(`[CACHE] Refresh interval updated: ${this.refreshIntervalMs}ms -> ${intervalMs}ms`);
    this.refreshIntervalMs = intervalMs;

    if (this.refreshTimer) {
      clearInterval(this.refreshTimer);
      this.refreshTimer = setInterval(() => {
        void this.performRefresh();
      }, this.refreshIntervalMs);
    }
  }
}

export const refreshScheduler = new RefreshScheduler();
