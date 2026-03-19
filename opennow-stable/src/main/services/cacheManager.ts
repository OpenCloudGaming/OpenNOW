import { app } from "electron";
import { mkdir, readFile, writeFile, unlink, readdir, rm } from "node:fs/promises";
import { existsSync } from "node:fs";
import { join } from "node:path";

interface CacheMetadata {
  timestamp: number;
  expiresAt: number;
}

interface CachedData<T> {
  data: T;
  metadata: CacheMetadata;
}

const CACHE_DIRECTORY = "gfn-cache";
const CACHE_TTL_MS = 12 * 60 * 60 * 1000;

const THUMBNAILS_DIRECTORY = "media-thumbs";

class CacheManager {
  private cacheDir: string;
  private initialized: boolean = false;

  constructor() {
    this.cacheDir = join(app.getPath("userData"), CACHE_DIRECTORY);
  }

  async initialize(): Promise<void> {
    if (this.initialized) return;
    try {
      await mkdir(this.cacheDir, { recursive: true });
      this.initialized = true;
      console.log(`[CACHE] Initialized cache directory: ${this.cacheDir}`);
    } catch (error) {
      console.error(`[CACHE] Failed to initialize cache directory:`, error);
      throw error;
    }
  }

  private getCacheFilePath(key: string): string {
    const sanitized = key.replace(/[^a-z0-9-]/gi, "_");
    return join(this.cacheDir, `${sanitized}.json`);
  }

  async loadFromCache<T>(key: string): Promise<CachedData<T> | null> {
    if (!this.initialized) {
      console.warn(`[CACHE] Cache not initialized, skipping load for key: ${key}`);
      return null;
    }

    const filePath = this.getCacheFilePath(key);

    if (!existsSync(filePath)) {
      console.log(`[CACHE] Cache miss (file not found): ${key}`);
      return null;
    }

    try {
      const content = await readFile(filePath, "utf-8");
      const parsed = JSON.parse(content) as CachedData<T>;

      if (!parsed.metadata || typeof parsed.metadata.expiresAt !== "number") {
        console.warn(`[CACHE] Cache corrupted (invalid metadata): ${key}`);
        await this.invalidateCache(key);
        return null;
      }

      const now = Date.now();
      if (now > parsed.metadata.expiresAt) {
        console.log(`[CACHE] Cache expired: ${key} (expired ${Math.round((now - parsed.metadata.expiresAt) / 1000)}s ago)`);
        return null;
      }

      const ageSeconds = Math.round((now - parsed.metadata.timestamp) / 1000);
      console.log(`[CACHE] Cache hit: ${key} (age: ${ageSeconds}s)`);
      return parsed;
    } catch (error) {
      console.error(`[CACHE] Error reading cache file: ${key}`, error);
      try {
        await this.invalidateCache(key);
      } catch (deleteError) {
        console.error(`[CACHE] Failed to delete corrupted cache file: ${key}`, deleteError);
      }
      return null;
    }
  }

  async saveToCache<T>(key: string, data: T): Promise<void> {
    if (!this.initialized) {
      console.warn(`[CACHE] Cache not initialized, skipping save for key: ${key}`);
      return;
    }

    const filePath = this.getCacheFilePath(key);
    const now = Date.now();
    const cached: CachedData<T> = {
      data,
      metadata: {
        timestamp: now,
        expiresAt: now + CACHE_TTL_MS,
      },
    };

    try {
      await writeFile(filePath, JSON.stringify(cached, null, 2), "utf-8");
      console.log(`[CACHE] Saved to cache: ${key}`);
    } catch (error) {
      console.error(`[CACHE] Error writing cache file: ${key}`, error);
      throw error;
    }
  }

  async invalidateCache(key: string): Promise<void> {
    const filePath = this.getCacheFilePath(key);

    if (!existsSync(filePath)) {
      console.log(`[CACHE] Cache already invalid or missing: ${key}`);
      return;
    }

    try {
      await unlink(filePath);
      console.log(`[CACHE] Invalidated cache: ${key}`);
    } catch (error) {
      console.error(`[CACHE] Error deleting cache file: ${key}`, error);
      throw error;
    }
  }

  async deleteAll(): Promise<void> {
    if (!this.initialized) {
      console.warn(`[CACHE] Cache not initialized, skipping deleteAll`);
      return;
    }

    try {
      const files = await readdir(this.cacheDir);
      for (const file of files) {
        const filePath = join(this.cacheDir, file);
        try {
          await unlink(filePath);
          console.log(`[CACHE] Deleted cache file: ${file}`);
        } catch (err) {
          console.error(`[CACHE] Error deleting cache file: ${file}`, err);
        }
      }
      console.log(`[CACHE] Cleared all cache files in ${this.cacheDir}`);

      // Also remove the thumbnail cache directory created by main process
      const thumbsDir = join(app.getPath("userData"), THUMBNAILS_DIRECTORY);
      try {
        await rm(thumbsDir, { recursive: true, force: true });
        console.log(`[CACHE] Removed thumbnail cache directory: ${thumbsDir}`);
      } catch (err) {
        // Non-fatal: log and continue
        console.warn(`[CACHE] Failed to remove thumbnail cache directory: ${thumbsDir}`, err);
      }
    } catch (error) {
      console.error(`[CACHE] Error clearing all cache:`, error);
      throw error;
    }
  }

  isExpired(timestamp: number): boolean {
    const ageMs = Date.now() - timestamp;
    return ageMs > CACHE_TTL_MS;
  }

  getCacheTtlMs(): number {
    return CACHE_TTL_MS;
  }
}

export const cacheManager = new CacheManager();
