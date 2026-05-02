import { IPC_CHANNELS } from "@shared/ipc";
import type { Settings } from "@shared/gfn";
import type { MainIpcDeps } from "./types";
import { connectDiscordRpc, destroyDiscordRpc } from "../discordRpc";

export function registerWindowUpdaterSettingsIpc(deps: MainIpcDeps): void {
  const { ipcMain, app, getMainWindow, settingsManager, appUpdater, requestAppShutdown, discordMonitor, systemPreferences } =
    deps;

  ipcMain.handle(IPC_CHANNELS.TOGGLE_FULLSCREEN, async () => {
    const win = getMainWindow();
    if (win && !win.isDestroyed()) {
      const isFullScreen = win.isFullScreen();
      win.setFullScreen(!isFullScreen);
    }
  });

  ipcMain.handle(IPC_CHANNELS.SET_FULLSCREEN, async (_event, value: boolean) => {
    const win = getMainWindow();
    if (win && !win.isDestroyed()) {
      try {
        win.setFullScreen(Boolean(value));
      } catch (err) {
        console.warn("Failed to set fullscreen:", err);
      }
    }
  });

  ipcMain.handle(IPC_CHANNELS.TOGGLE_POINTER_LOCK, async () => {
    const win = getMainWindow();
    if (win && !win.isDestroyed()) {
      win.webContents.send("app:toggle-pointer-lock");
    }
  });

  ipcMain.handle(IPC_CHANNELS.QUIT_APP, async () => {
    requestAppShutdown({
      reason: "renderer-explicit-exit",
      forceExitFallback: true,
    });
  });

  ipcMain.handle(IPC_CHANNELS.APP_UPDATER_GET_STATE, async () => {
    return appUpdater?.getState() ?? {
      status: "disabled",
      currentVersion: app.getVersion(),
      updateSource: "github-releases",
      canCheck: false,
      canDownload: false,
      canInstall: false,
      isPackaged: app.isPackaged,
      message: "Updater is unavailable.",
    };
  });

  ipcMain.handle(IPC_CHANNELS.APP_UPDATER_CHECK, async () => {
    return appUpdater?.checkForUpdates("manual") ?? {
      status: "disabled",
      currentVersion: app.getVersion(),
      updateSource: "github-releases",
      canCheck: false,
      canDownload: false,
      canInstall: false,
      isPackaged: app.isPackaged,
      message: "Updater is unavailable.",
    };
  });

  ipcMain.handle(IPC_CHANNELS.APP_UPDATER_DOWNLOAD, async () => {
    return appUpdater?.downloadUpdate() ?? {
      status: "disabled",
      currentVersion: app.getVersion(),
      updateSource: "github-releases",
      canCheck: false,
      canDownload: false,
      canInstall: false,
      isPackaged: app.isPackaged,
      message: "Updater is unavailable.",
    };
  });

  ipcMain.handle(IPC_CHANNELS.APP_UPDATER_INSTALL, async () => {
    return appUpdater?.quitAndInstall() ?? {
      status: "disabled",
      currentVersion: app.getVersion(),
      updateSource: "github-releases",
      canCheck: false,
      canDownload: false,
      canInstall: false,
      isPackaged: app.isPackaged,
      message: "Updater is unavailable.",
    };
  });

  ipcMain.handle(IPC_CHANNELS.SETTINGS_GET, async (): Promise<Settings> => {
    return settingsManager.getAll();
  });

  ipcMain.handle(
    IPC_CHANNELS.SETTINGS_SET,
    async <K extends keyof Settings>(_event: Electron.IpcMainInvokeEvent, key: K, value: Settings[K]) => {
      settingsManager.set(key, value);
      try {
        if (key === "autoCheckForUpdates") {
          appUpdater?.setAutomaticChecksEnabled(value as boolean);
        }
        if (key === "discordRichPresence") {
          if (value) {
            void connectDiscordRpc().then(() => discordMonitor.start());
          } else {
            discordMonitor.stop();
            void destroyDiscordRpc();
          }
        }
      } catch (err) {
        console.warn("Failed to apply setting change in main process:", err);
      }
    },
  );

  ipcMain.handle(IPC_CHANNELS.SETTINGS_RESET, async (): Promise<Settings> => {
    const resetSettings = settingsManager.reset();
    appUpdater?.setAutomaticChecksEnabled(resetSettings.autoCheckForUpdates);
    return resetSettings;
  });

  ipcMain.handle(IPC_CHANNELS.MICROPHONE_PERMISSION_GET, async () => {
    if (process.platform !== "darwin") {
      return {
        platform: process.platform,
        isMacOs: false,
        status: "not-applicable" as const,
        granted: false,
        canRequest: false,
        shouldUseBrowserApi: true,
      };
    }

    const currentStatus = systemPreferences.getMediaAccessStatus("microphone");
    console.log("[Main] macOS microphone permission status:", currentStatus);

    if (currentStatus === "granted") {
      return {
        platform: process.platform,
        isMacOs: true,
        status: "granted" as const,
        granted: true,
        canRequest: false,
        shouldUseBrowserApi: true,
      };
    }

    if (currentStatus === "not-determined") {
      const granted = await systemPreferences.askForMediaAccess("microphone");
      const nextStatus = systemPreferences.getMediaAccessStatus("microphone");
      console.log("[Main] Requested macOS microphone permission:", granted, nextStatus);
      return {
        platform: process.platform,
        isMacOs: true,
        status: nextStatus,
        granted,
        canRequest: nextStatus === "not-determined",
        shouldUseBrowserApi: granted,
      };
    }

    return {
      platform: process.platform,
      isMacOs: true,
      status: currentStatus,
      granted: false,
      canRequest: false,
      shouldUseBrowserApi: false,
    };
  });

  ipcMain.handle(IPC_CHANNELS.LOGS_EXPORT, async (_event, format: "text" | "json" = "text"): Promise<string> => {
    return deps.exportLogs(format);
  });
}
