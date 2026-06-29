import type { GameInfo } from "@shared/gfn";
import { getAccountGamesCacheKeys } from "../gfn/games";
import { sessionProxyHasCredentials } from "../gfn/proxyUrl";
import { cacheEventBus } from "./cacheEventBus";
import { cacheManager } from "./cacheManager";

export interface RefreshAuthContext {
  token: string;
  userId: string;
  providerStreamingBaseUrl?: string;
  proxyUrl?: string;
}

type FetchFunction<T> = (
  token: string,
  providerStreamingBaseUrl?: string,
  proxyUrl?: string,
) => Promise<T>;
type PublicFetchFunction = (proxyUrl?: string) => Promise<GameInfo[]>;

class RefreshScheduler {
  private refreshTimer: ReturnType<typeof setInterval> | null = null;
  private isRefreshing: boolean = false;
  private authContext: RefreshAuthContext | null = null;
  private fetchMainGamesUncached: FetchFunction<GameInfo[]> | null = null;
  private fetchLibraryGamesUncached: FetchFunction<GameInfo[]> | null = null;
  private fetchPublicGamesUncached: PublicFetchFunction | null = null;
  private refreshIntervalMs: number = 12 * 60 * 60 * 1000;

  initialize(
    fetchMainGamesUncached: FetchFunction<GameInfo[]>,
    fetchLibraryGamesUncached: FetchFunction<GameInfo[]>,
    fetchPublicGamesUncached: PublicFetchFunction,
  ): void {
    this.fetchMainGamesUncached = fetchMainGamesUncached;
    this.fetchLibraryGamesUncached = fetchLibraryGamesUncached;
    this.fetchPublicGamesUncached = fetchPublicGamesUncached;
    console.log(`[CACHE] RefreshScheduler initialized (interval: ${this.refreshIntervalMs / 60000} minutes)`);
  }

  updateAuthContext(token: string, userId: string, providerStreamingBaseUrl?: string, proxyUrl?: string): void {
    this.authContext = { token, userId, providerStreamingBaseUrl, proxyUrl };
    console.log(`[CACHE] Auth context updated for refresh scheduler`);
  }

  start(): void {
    if (this.refreshTimer) {
      console.warn(`[CACHE] RefreshScheduler already started`);
      return;
    }

    if (!this.fetchMainGamesUncached || !this.fetchLibraryGamesUncached || !this.fetchPublicGamesUncached) {
      console.error(`[CACHE] Cannot start RefreshScheduler: fetch functions not initialized`);
      return;
    }

    console.log(`[CACHE] Starting RefreshScheduler`);
    void this.performRefresh();
    this.refreshTimer = setInterval(() => {
      void this.performRefresh();
    }, this.refreshIntervalMs);
    this.refreshTimer.unref?.();
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

  async performRefresh(options: { force?: boolean } = {}): Promise<void> {
    if (this.isRefreshing) {
      console.log(`[CACHE] Refresh already in progress, skipping`);
      return;
    }

    if (!this.authContext) {
      console.log(`[CACHE] Auth context not available, skipping refresh`);
      return;
    }

    if (!this.fetchMainGamesUncached || !this.fetchLibraryGamesUncached || !this.fetchPublicGamesUncached) {
      console.error(`[CACHE] Fetch functions not available`);
      return;
    }

    const { token, userId, providerStreamingBaseUrl, proxyUrl } = this.authContext;
    if (sessionProxyHasCredentials(proxyUrl)) {
      console.log("[CACHE] Credentialed proxy configured, skipping background game cache refresh");
      return;
    }

    const cacheKeys = getAccountGamesCacheKeys(userId, providerStreamingBaseUrl, proxyUrl);
    const force = options.force === true;

    const [mainNeedsRefresh, libraryNeedsRefresh, publicNeedsRefresh] = force
      ? [true, true, true]
      : await Promise.all([
        cacheManager.isStaleOrMissing(cacheKeys.main),
        cacheManager.isStaleOrMissing(cacheKeys.library),
        cacheManager.isStaleOrMissing(cacheKeys.public),
      ]);

    if (!mainNeedsRefresh && !libraryNeedsRefresh && !publicNeedsRefresh) {
      console.log("[CACHE] All game caches are fresh, skipping background refresh");
      return;
    }

    this.isRefreshing = true;
    const startTime = Date.now();
    console.log("[CACHE] Refresh cycle started", {
      main: mainNeedsRefresh,
      library: libraryNeedsRefresh,
      public: publicNeedsRefresh,
      force,
    });

    try {
      cacheEventBus.emit("cache:refresh-start");

      const refreshTasks: Promise<void>[] = [];

      if (mainNeedsRefresh) {
        refreshTasks.push(
          this.fetchMainGamesUncached(token, providerStreamingBaseUrl, proxyUrl)
            .then(async (games) => {
              await cacheManager.saveToCache(cacheKeys.main, games);
            }),
        );
      }

      if (libraryNeedsRefresh) {
        refreshTasks.push(
          this.fetchLibraryGamesUncached(token, providerStreamingBaseUrl, proxyUrl)
            .then(async (games) => {
              await cacheManager.saveToCache(cacheKeys.library, games);
            }),
        );
      }

      if (publicNeedsRefresh) {
        refreshTasks.push(
          this.fetchPublicGamesUncached(proxyUrl)
            .then(async (games) => {
              await cacheManager.saveToCache(cacheKeys.public, games);
            }),
        );
      }

      const results = await Promise.allSettled(refreshTasks);

      let hasErrors = false;
      const taskNames: string[] = [];
      if (mainNeedsRefresh) taskNames.push("main");
      if (libraryNeedsRefresh) taskNames.push("library");
      if (publicNeedsRefresh) taskNames.push("public");

      for (let i = 0; i < results.length; i += 1) {
        const result = results[i];
        if (result.status === "rejected") {
          hasErrors = true;
          const name = taskNames[i] ?? "unknown";
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
    await this.performRefresh({ force: true });
  }

  setRefreshInterval(intervalMs: number): void {
    console.log(`[CACHE] Refresh interval updated: ${this.refreshIntervalMs}ms -> ${intervalMs}ms`);
    this.refreshIntervalMs = intervalMs;

    if (this.refreshTimer) {
      clearInterval(this.refreshTimer);
      this.refreshTimer = setInterval(() => {
        void this.performRefresh();
      }, this.refreshIntervalMs);
      this.refreshTimer.unref?.();
    }
  }
}

export const refreshScheduler = new RefreshScheduler();
