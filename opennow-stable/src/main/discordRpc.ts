import { Client } from "discord-rpc";

/**
 * Discord Application ID for OpenNOW.
 * Register an application at https://discord.com/developers/applications
 * and paste its Client ID here.
 */
const DISCORD_CLIENT_ID = "1479944467112001669";

let rpcClient: Client | null = null;
let connected = false;
let pendingActivity: { gameName: string; startTimestamp: Date } | null = null;

/**
 * Initialise and connect the Discord RPC client.
 * Errors are swallowed so that a missing or closed Discord installation
 * never crashes or blocks the rest of the application.
 */
export async function connectDiscordRpc(): Promise<void> {
  if (rpcClient) return;

  const client = new Client({ transport: "ipc" });

  client.on("disconnected", () => {
    connected = false;
    rpcClient = null;
    console.log("[DiscordRPC] Disconnected.");
  });

  try {
    await client.login({ clientId: DISCORD_CLIENT_ID });
    rpcClient = client;
    connected = true;
    console.log("[DiscordRPC] Connected.");

    if (pendingActivity) {
      await setActivity(pendingActivity.gameName, pendingActivity.startTimestamp);
      pendingActivity = null;
    }
  } catch (err) {
    console.warn("[DiscordRPC] Failed to connect (Discord may not be running):", (err as Error).message);
    rpcClient = null;
    connected = false;
  }
}

/**
 * Update the Discord "Now Playing" activity to show the given game name and
 * how long the user has been playing.
 */
export async function setActivity(gameName: string, startTimestamp: Date): Promise<void> {
  if (!connected || !rpcClient) {
    pendingActivity = { gameName, startTimestamp };
    return;
  }

  try {
    await rpcClient.setActivity({
      details: gameName,
      state: "Streaming via OpenNow",
      startTimestamp,
      instance: false,
    });
    pendingActivity = null;
  } catch (err) {
    console.warn("[DiscordRPC] setActivity failed:", (err as Error).message);
  }
}

/**
 * Clear the Discord activity (call when a stream ends or the app quits).
 */
export async function clearActivity(): Promise<void> {
  pendingActivity = null;

  if (!connected || !rpcClient) return;

  try {
    await rpcClient.clearActivity();
  } catch (err) {
    console.warn("[DiscordRPC] clearActivity failed:", (err as Error).message);
  }
}

/**
 * Destroy the RPC connection gracefully (call on app quit).
 */
export async function destroyDiscordRpc(): Promise<void> {
  pendingActivity = null;

  if (!rpcClient) return;

  try {
    await rpcClient.destroy();
  } catch {
    // Ignore errors during teardown
  } finally {
    rpcClient = null;
    connected = false;
  }
}
