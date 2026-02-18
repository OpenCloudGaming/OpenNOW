import { Client } from "@xhayper/discord-rpc";
import type { DiscordPresencePayload } from "@shared/gfn";

const RECONNECT_DELAY_MS = 15_000;
const MAX_RECONNECT_DELAY_MS = 120_000;

export class DiscordPresenceService {
  private client: Client | null = null;
  private clientId: string;
  private enabled: boolean;
  private connected = false;
  private disposed = false;
  private reconnectTimer: NodeJS.Timeout | null = null;
  private reconnectAttempt = 0;
  private lastPayload: DiscordPresencePayload | null = null;

  constructor(enabled: boolean, clientId: string) {
    this.enabled = enabled;
    this.clientId = clientId;
  }

  async initialize(): Promise<void> {
    if (!this.enabled || !this.clientId) {
      return;
    }
    await this.connect();
  }

  private async connect(): Promise<void> {
    if (this.disposed || !this.enabled || !this.clientId) {
      return;
    }

    this.clearReconnectTimer();

    try {
      const client = new Client({ clientId: this.clientId });

      client.on("ready", () => {
        console.log("[Discord] RPC connected");
        this.connected = true;
        this.reconnectAttempt = 0;
        if (this.lastPayload) {
          void this.setActivity(this.lastPayload);
        }
      });

      client.on("disconnected", () => {
        console.log("[Discord] RPC disconnected");
        this.connected = false;
        this.client = null;
        this.scheduleReconnect();
      });

      await client.login();
      this.client = client;
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      if (msg.includes("ENOENT") || msg.includes("Could not connect")) {
        console.log("[Discord] Discord not running, will retry later");
      } else {
        console.warn("[Discord] RPC connect failed:", msg);
      }
      this.connected = false;
      this.client = null;
      this.scheduleReconnect();
    }
  }

  private scheduleReconnect(): void {
    if (this.disposed || !this.enabled || !this.clientId) {
      return;
    }

    this.clearReconnectTimer();
    const delay = Math.min(
      RECONNECT_DELAY_MS * 2 ** this.reconnectAttempt,
      MAX_RECONNECT_DELAY_MS,
    );
    this.reconnectAttempt += 1;
    console.log(`[Discord] Reconnecting in ${Math.round(delay / 1000)}s (attempt ${this.reconnectAttempt})`);
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      void this.connect();
    }, delay);
  }

  private clearReconnectTimer(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  async updatePresence(payload: DiscordPresencePayload): Promise<void> {
    this.lastPayload = payload;

    if (!this.enabled || !this.clientId) {
      return;
    }

    if (!this.connected || !this.client) {
      return;
    }

    await this.setActivity(payload);
  }

  private async setActivity(payload: DiscordPresencePayload): Promise<void> {
    if (!this.client || !this.connected) {
      return;
    }

    try {
      if (payload.type === "idle") {
        await this.client.user?.setActivity({
          details: "Browsing library",
          state: "Idle",
          largeImageKey: "opennow",
          largeImageText: "OpenNOW",
        });
      } else if (payload.type === "queue") {
        const state = payload.queuePosition
          ? `Position #${payload.queuePosition}`
          : "Waiting";
        await this.client.user?.setActivity({
          details: payload.gameName ? `In queue — ${payload.gameName}` : "In queue",
          state,
          largeImageKey: "opennow",
          largeImageText: "OpenNOW",
          ...(payload.startTimestamp ? { startTimestamp: new Date(payload.startTimestamp) } : {}),
        });
      } else if (payload.type === "streaming") {
        const name = payload.gameName?.trim();
        const details = name ? `Streaming ${name}` : "Streaming";

        const stateParts: string[] = [];
        if (payload.resolution && payload.fps) {
          stateParts.push(`${payload.resolution} @ ${payload.fps}fps`);
        } else if (payload.resolution) {
          stateParts.push(payload.resolution);
        }
        if (payload.bitrateMbps && payload.bitrateMbps > 0) {
          stateParts.push(`${payload.bitrateMbps.toFixed(1)} Mbps`);
        }
        if (payload.region) {
          stateParts.push(payload.region);
        }
        const state = stateParts.length > 0 ? stateParts.join(" · ") : undefined;

        await this.client.user?.setActivity({
          details,
          ...(state ? { state } : {}),
          largeImageKey: "opennow",
          largeImageText: "OpenNOW",
          ...(payload.startTimestamp ? { startTimestamp: new Date(payload.startTimestamp) } : {}),
        });
      }
    } catch (error) {
      console.warn("[Discord] Failed to set activity:", error instanceof Error ? error.message : error);
    }
  }

  async clearPresence(): Promise<void> {
    this.lastPayload = null;

    if (!this.client || !this.connected) {
      return;
    }

    try {
      await this.client.user?.clearActivity();
    } catch (error) {
      console.warn("[Discord] Failed to clear activity:", error instanceof Error ? error.message : error);
    }
  }

  async updateConfig(enabled: boolean, clientId: string): Promise<void> {
    const wasEnabled = this.enabled;
    const oldClientId = this.clientId;

    this.enabled = enabled;
    this.clientId = clientId;

    if (!enabled) {
      await this.clearPresence();
      await this.disconnect();
      return;
    }

    if (enabled && (!wasEnabled || clientId !== oldClientId)) {
      await this.disconnect();
      this.disposed = false;
      this.reconnectAttempt = 0;
      await this.connect();
      return;
    }
  }

  private async disconnect(): Promise<void> {
    this.clearReconnectTimer();
    this.connected = false;

    if (this.client) {
      try {
        await this.client.destroy();
      } catch {
        // ignore
      }
      this.client = null;
    }
  }

  async dispose(): Promise<void> {
    this.disposed = true;
    this.lastPayload = null;
    await this.disconnect();
  }
}
