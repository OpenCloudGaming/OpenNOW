import { app } from "electron";
import { join } from "node:path";
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import type { FlightProfile } from "@shared/gfn";
import { buildDefaultProfile } from "@shared/flightDefaults";

const PROFILES_FILENAME = "flight-profiles.json";

interface ProfileStore {
  profiles: FlightProfile[];
}

export class FlightProfileManager {
  private readonly profilesPath: string;
  private store: ProfileStore;

  constructor() {
    this.profilesPath = join(app.getPath("userData"), PROFILES_FILENAME);
    this.store = this.load();
  }

  private load(): ProfileStore {
    try {
      if (!existsSync(this.profilesPath)) {
        return { profiles: [] };
      }
      const content = readFileSync(this.profilesPath, "utf-8");
      const parsed = JSON.parse(content) as Partial<ProfileStore>;
      return { profiles: Array.isArray(parsed.profiles) ? parsed.profiles : [] };
    } catch (error) {
      console.error("[Flight] Failed to load profiles:", error);
      return { profiles: [] };
    }
  }

  private save(): void {
    try {
      const dir = join(app.getPath("userData"));
      if (!existsSync(dir)) {
        mkdirSync(dir, { recursive: true });
      }
      writeFileSync(this.profilesPath, JSON.stringify(this.store, null, 2), "utf-8");
    } catch (error) {
      console.error("[Flight] Failed to save profiles:", error);
    }
  }

  getProfile(vidPid: string, gameId?: string): FlightProfile | null {
    if (gameId) {
      const gameProfile = this.store.profiles.find(
        (p) => p.vidPid === vidPid && p.gameId === gameId,
      );
      if (gameProfile) return gameProfile;
    }
    return this.store.profiles.find(
      (p) => p.vidPid === vidPid && !p.gameId,
    ) ?? null;
  }

  setProfile(profile: FlightProfile): void {
    const idx = this.store.profiles.findIndex(
      (p) => p.vidPid === profile.vidPid && p.gameId === profile.gameId,
    );
    if (idx >= 0) {
      this.store.profiles[idx] = profile;
    } else {
      this.store.profiles.push(profile);
    }
    this.save();
  }

  deleteProfile(vidPid: string, gameId?: string): void {
    this.store.profiles = this.store.profiles.filter(
      (p) => !(p.vidPid === vidPid && p.gameId === gameId),
    );
    this.save();
  }

  getAllProfiles(): FlightProfile[] {
    return [...this.store.profiles];
  }

  resetProfile(vidPid: string): FlightProfile | null {
    const parts = vidPid.split(":");
    if (parts.length !== 2) return null;
    const vendorId = parseInt(parts[0]!, 16);
    const productId = parseInt(parts[1]!, 16);
    if (!Number.isFinite(vendorId) || !Number.isFinite(productId)) return null;

    this.store.profiles = this.store.profiles.filter(
      (p) => !(p.vidPid === vidPid && !p.gameId),
    );

    const defaultProfile = buildDefaultProfile(vendorId, productId, "");
    this.store.profiles.push(defaultProfile);
    this.save();
    return defaultProfile;
  }

  getOrCreateProfile(vendorId: number, productId: number, deviceName: string): FlightProfile {
    const vidPid = `${vendorId.toString(16).toUpperCase().padStart(4, "0")}:${productId.toString(16).toUpperCase().padStart(4, "0")}`;
    const existing = this.getProfile(vidPid);
    if (existing) return existing;

    const profile = buildDefaultProfile(vendorId, productId, deviceName);
    this.store.profiles.push(profile);
    this.save();
    return profile;
  }
}
