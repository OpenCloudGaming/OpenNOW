import { callable } from "@decky/api";

export type InstallKind = "flatpak" | "appimage" | "native" | "custom";
export type Codec = "auto" | "av1" | "h265" | "h264";
export type PreferenceKey = "consoleLayout" | "collection" | "gamePageButton" | "nightlyChannel";

export interface Install {
  kind: InstallKind;
  path: string;
  version: string | null;
  exe: string;
  args: string[];
  data_dir: string;
}

export interface Release {
  version: string;
  asset: { name: string; url: string; size: number } | null;
  installedVersionKnown: boolean;
  updateAvailable: boolean;
}

export interface InstallProgress {
  phase: "download" | "install";
  fraction: number;
  version: string;
}

export interface Status {
  install: Install | null;
  running: boolean;
  signedIn: boolean;
  release: Release | null;
  installProgress: InstallProgress | null;
  pluginVersion: string;
  logPath: string;
}

export interface AddedGame {
  appId: number;
  variantId: string;
  title: string;
  store: string;
  art: string;
  addedAt: number;
}

export interface PluginState {
  customPath: string;
  launcherAppId: number | null;
  games: AddedGame[];
  preferences: Record<PreferenceKey, boolean>;
}

export interface StreamValues {
  resolution: string;
  fps: number;
  maxBitrateMbps: number;
  codec: Codec;
  enableHdr: boolean;
  identifyAsSteamDeck: boolean;
  steamBigPictureMode: boolean;
}

export interface StreamSettings {
  values: StreamValues;
  options: { resolutions: string[]; frameRates: number[]; codecs: Codec[]; bitrate: [number, number] };
  locked: boolean;
  error: string | null;
}

export interface CatalogVariant {
  id: string;
  store: string;
  storeId: string;
}

export interface CatalogArt {
  grid: string;
  wide: string;
  hero: string;
  logo: string;
}

export interface CatalogGame {
  id: string;
  title: string;
  variants: CatalogVariant[];
  art: CatalogArt;
}

export interface CatalogMatch {
  game: CatalogGame;
  variant: CatalogVariant;
}

export interface LaunchCommand {
  exe: string;
  startDir: string;
  options: string;
}

export const backend = {
  getStatus: callable<[], Status>("get_status"),
  getState: callable<[], PluginState>("get_state"),
  setPreference: callable<[key: PreferenceKey, value: boolean], PluginState>("set_preference"),
  setCustomPath: callable<[path: string], PluginState>("set_custom_path"),
  setLauncher: callable<[appId: number | null], PluginState>("set_launcher"),
  addGame: callable<[appId: number, variantId: string, title: string, store: string, art: string], PluginState>("add_game"),
  removeGame: callable<[appId: number], PluginState>("remove_game"),
  launchCommand: callable<[variantId?: string, title?: string], LaunchCommand>("launch_command"),
  getStreamSettings: callable<[], StreamSettings>("get_stream_settings"),
  setStreamSetting: callable<[key: keyof StreamValues, value: StreamValues[keyof StreamValues]], StreamValues>("set_stream_setting"),
  searchCatalog: callable<[query: string], CatalogGame[]>("search_catalog"),
  refreshCatalog: callable<[], number>("refresh_catalog"),
  gamePageMatches: callable<[steamAppId: number, shortcutTitle: string], CatalogMatch[]>("game_page_matches"),
  artwork: callable<[url: string], { data: string; type: "png" | "jpg" }>("artwork"),
  checkUpdate: callable<[], Release>("check_update"),
  installOpenNOW: callable<[nightly?: boolean], boolean>("install_opennow"),
  cancelInstall: callable<[], boolean>("cancel_install"),
  closeOpenNOW: callable<[], boolean>("close_opennow"),
};
