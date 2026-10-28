import { ConfirmModal, Navigation, Router, showModal, sleep } from "@decky/ui";
import { toaster } from "@decky/api";
import { backend, CatalogGame, CatalogVariant, PluginState } from "./backend";
import * as steam from "./steam";
import { storeLabel } from "./stores";
import { refresh, store } from "./store";

const LAUNCHER_NAME = "OpenNOW";

export function ownedShortcutIds(state: PluginState): number[] {
  return [state.launcherAppId, ...state.games.map((game) => game.appId)].filter((id): id is number => id !== null);
}

export function runningShortcut(state: PluginState): number | null {
  return ownedShortcutIds(state).find((appId) => steam.isRunning(appId)) ?? null;
}

export async function ensureLauncher(): Promise<number> {
  const state = await backend.getState();
  const command = await backend.launchCommand();
  if (state.launcherAppId !== null && steam.overview(state.launcherAppId) !== null) {
    steam.applyLaunchCommand(state.launcherAppId, LAUNCHER_NAME, command);
    return state.launcherAppId;
  }
  const appId = await steam.createShortcut(LAUNCHER_NAME, command);
  if (state.preferences.collection) await steam.addToCollection(appId);
  await backend.setLauncher(appId);
  await refresh();
  return appId;
}

export async function recreateLauncher() {
  const state = store.get().state ?? (await backend.getState());
  if (state.launcherAppId !== null) {
    await steam.removeShortcut(state.launcherAppId);
    await backend.setLauncher(null);
    await refresh();
  }
  await ensureLauncher();
  toaster.toast({ title: "OpenNOW", body: "Re-created OpenNOW in your library" });
}

let launcherVariantId: string | null = null;

export async function launchOpenNOW() {
  const launcher = await ensureLauncher();
  if (!steam.isRunning(launcher)) launcherVariantId = null;
  steam.launch(launcher);
}

function chooseOpenGame(title: string): Promise<"resume" | "restart" | "cancel"> {
  return new Promise((resolve) => {
    showModal(
      <ConfirmModal
        strTitle={title + " is open in OpenNOW"}
        strDescription="Go back to it, or start it again. Starting again ends the stream if it's still running."
        strOKButtonText="Go to OpenNOW"
        strMiddleButtonText="Start again"
        onOK={() => resolve("resume")}
        onMiddleButton={() => resolve("restart")}
        onCancel={() => resolve("cancel")}
      />,
    );
  });
}

const EXIT_TIMEOUT_MS = 15000;
const START_TIMEOUT_MS = 30000;

async function stopOpenNOW(): Promise<boolean> {
  const state = store.get().state;
  const running = state ? runningShortcut(state) : null;
  if (running !== null) Router.NavigateToRunningApp(false);
  Navigation.CloseSideMenus();
  if (!(await backend.closeOpenNOW())) return false;
  if (running !== null && !(await steam.waitForExit(running, EXIT_TIMEOUT_MS))) {
    throw new Error("OpenNOW didn't close. Try again.");
  }
  await refresh();
  if (store.get().status?.running) throw new Error("OpenNOW didn't close. Try again.");
  return true;
}

async function endSessionFor(): Promise<boolean> {
  const state = store.get().state;
  if ((state ? runningShortcut(state) : null) === null && !store.get().status?.running) return true;
  return stopOpenNOW();
}

export async function launchGame(appId: number) {
  await refresh();
  const state = store.get().state;
  if (state && runningShortcut(state) === appId) {
    Navigation.CloseSideMenus();
    return;
  }
  if (await endSessionFor()) steam.launch(appId);
}

export async function playOnOpenNOW(game: CatalogGame, variant: CatalogVariant) {
  const launcher = await ensureLauncher();
  await refresh();
  if (steam.isRunning(launcher) && launcherVariantId === variant.id) {
    const choice = await chooseOpenGame(game.title);
    if (choice === "cancel") return;
    if (choice === "resume") {
      steam.launch(launcher);
      return;
    }
    if (!(await stopOpenNOW())) return;
  } else if (!(await endSessionFor())) return;
  steam.applyLaunchCommand(launcher, LAUNCHER_NAME, await backend.launchCommand(variant.id, game.title));
  try {
    steam.launch(launcher);
    const deadline = Date.now() + START_TIMEOUT_MS;
    while (Date.now() < deadline && !(await backend.getStatus()).running) await sleep(1000);
  } finally {
    steam.applyLaunchCommand(launcher, LAUNCHER_NAME, await backend.launchCommand());
  }
  if (!(await backend.getStatus()).running) throw new Error("OpenNOW didn't start. Try again.");
  launcherVariantId = variant.id;
}

export async function closeOpenNOW() {
  await stopOpenNOW();
}

export function shortcutName(game: CatalogGame, variant: CatalogVariant): string {
  return game.variants.length > 1 ? game.title + " (" + storeLabel(variant.store) + ")" : game.title;
}

export async function addGame(game: CatalogGame, variant: CatalogVariant): Promise<number> {
  const existing = store.get().state?.games.find((entry) => entry.variantId === variant.id);
  if (existing && steam.overview(existing.appId) !== null) return existing.appId;
  const name = shortcutName(game, variant);
  const appId = await steam.createShortcut(name, await backend.launchCommand(variant.id, game.title));
  try {
    await steam.setCatalogArtwork(appId, game.art);
  } catch (error) {
    console.warn("OpenNOW: artwork failed", error);
  }
  if (store.get().state?.preferences.collection) await steam.addToCollection(appId);
  await backend.addGame(appId, variant.id, name, variant.store, game.art.grid);
  await refresh();
  toaster.toast({ title: "OpenNOW", body: "Added " + name + " to your library" });
  return appId;
}

export async function removeGame(appId: number) {
  await steam.removeShortcut(appId);
  await backend.removeGame(appId);
  await refresh();
}

export async function syncShortcuts() {
  const state = await backend.getState();
  const status = await backend.getStatus();
  if (!status.install) return;
  for (const game of state.games) {
    if (steam.overview(game.appId) === null) {
      await backend.removeGame(game.appId);
      continue;
    }
    steam.applyLaunchCommand(game.appId, game.title, await backend.launchCommand(game.variantId, game.title));
  }
  if (state.launcherAppId !== null && steam.overview(state.launcherAppId) !== null) {
    steam.applyLaunchCommand(state.launcherAppId, LAUNCHER_NAME, await backend.launchCommand());
  }
  await refresh();
}
