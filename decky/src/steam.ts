import { Router, sleep } from "@decky/ui";
import { ELibraryAssetType } from "@decky/ui/dist/globals/steam-client/App";
import { backend, CatalogArt, LaunchCommand } from "./backend";

export const COLLECTION = "OpenNOW";

interface Collection {
  apps: { has(appId: number): boolean };
  AsDragDropCollection(): { AddApps(overviews: unknown[]): void; RemoveApps(overviews: unknown[]): void };
  Save(): Promise<void>;
}

interface CollectionStore {
  GetCollectionIDByUserTag(tag: string): string | null;
  GetCollection(id: string): Collection | undefined;
  NewUnsavedCollection(tag: string, filter: undefined, overviews: unknown[]): Collection | undefined;
}

function collectionStore(): CollectionStore | null {
  return (window as unknown as { collectionStore?: CollectionStore }).collectionStore ?? null;
}

export function overview(appId: number) {
  return window.appStore.GetAppOverviewByAppID(appId);
}

async function waitFor(predicate: () => boolean, attempts = 20, delay = 100): Promise<boolean> {
  for (let attempt = 0; attempt < attempts; attempt++) {
    if (predicate()) return true;
    await sleep(delay);
  }
  return predicate();
}

export function applyLaunchCommand(appId: number, name: string, command: LaunchCommand) {
  window.SteamClient.Apps.SetShortcutName(appId, name);
  window.SteamClient.Apps.SetShortcutExe(appId, command.exe);
  window.SteamClient.Apps.SetShortcutStartDir(appId, command.startDir);
  window.SteamClient.Apps.SetShortcutLaunchOptions(appId, command.options);
}

export async function createShortcut(name: string, command: LaunchCommand): Promise<number> {
  const appId = await window.SteamClient.Apps.AddShortcut(name, command.exe, command.startDir, command.options);
  if (typeof appId !== "number" || !(await waitFor(() => overview(appId) !== null))) {
    throw new Error("Steam didn't create the shortcut. Restart Steam and try again.");
  }
  applyLaunchCommand(appId, name, command);
  return appId;
}

export async function removeShortcut(appId: number) {
  if (overview(appId) === null) return;
  window.SteamClient.Apps.RemoveShortcut(appId);
  await waitFor(() => overview(appId) === null);
}

const ART_SLOTS: [keyof CatalogArt, ELibraryAssetType][] = [
  ["grid", 0 as ELibraryAssetType.Capsule],
  ["hero", 1 as ELibraryAssetType.Hero],
  ["logo", 2 as ELibraryAssetType.Logo],
  ["wide", 3 as ELibraryAssetType.Header],
];

export async function setCatalogArtwork(appId: number, art: CatalogArt) {
  await Promise.all(ART_SLOTS.filter(([key]) => art[key]).map(async ([key, slot]) => {
    const image = await backend.artwork(art[key]);
    await window.SteamClient.Apps.SetCustomArtworkForApp(appId, image.data, image.type, slot);
  }));
}

export async function addToCollection(appId: number) {
  const store = collectionStore();
  const app = overview(appId);
  if (!store || !app) return;
  const existing = store.GetCollectionIDByUserTag(COLLECTION);
  const collection = existing ? store.GetCollection(existing) : store.NewUnsavedCollection(COLLECTION, undefined, []);
  if (!collection || collection.apps.has(appId)) return;
  collection.AsDragDropCollection().AddApps([app]);
  await collection.Save();
}

export function isRunning(appId: number | null): boolean {
  return appId !== null && Router.RunningApps.some((app) => Number(app.appid) === appId);
}

export function launch(appId: number) {
  const app = overview(appId);
  if (!app) throw new Error("That shortcut is gone from your library.");
  window.SteamClient.Apps.RunGame(app.gameid, "", -1, 100);
}

export function terminate(appId: number) {
  const app = overview(appId);
  if (app) window.SteamClient.Apps.TerminateApp(app.gameid, false);
}

export async function waitForExit(appId: number, timeoutMs: number): Promise<boolean> {
  return waitFor(() => !isRunning(appId), Math.ceil(timeoutMs / 200), 200);
}

export function onAppLifetime(callback: (appId: number, running: boolean) => void): () => void {
  const registration = window.SteamClient.GameSessions.RegisterForAppLifetimeNotifications((notification) => {
    callback(notification.unAppID, notification.bRunning);
  });
  return () => registration.unregister();
}
