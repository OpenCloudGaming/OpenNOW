import type { ControllerThemeStyle, GameInfo, MediaListingEntry, Settings } from "@shared/gfn";
import type {
  GameSubcategory,
  GamesHubReturnSnapshot,
  HomeRootPlane,
  LibrarySortId,
  MediaSubcategory,
  SettingsSubcategory,
  SoundKind,
  SpotlightEntry,
  TopCategory,
} from "../types";

export type DisplayItem = { id: string; label: string; value?: string };
export type OptionEntry = { id: string; label: string };

export interface OptionsActionContext {
  gamesShelfBrowseActive: boolean;
  mediaShelfBrowseActive: boolean;
  topCategory: TopCategory;
  gameSubcategory: GameSubcategory;
  gamesRootPlane: "spotlight" | "categories";
  /** When false, Games root has no spotlight row. */
  gamesDualShelf?: boolean;
  spotlightEntries: SpotlightEntry[];
  spotlightIndex: number;
  selectedMediaIndex: number;
  mediaAssetItems: MediaListingEntry[];
  selectedGame: GameInfo | null;
  /** When Game Hub is open from Home, the focused title (Games browse uses `selectedGame`). */
  gamesHubDisplayGame?: GameInfo | null;
  gamesHubOpen?: boolean;
  currentStreamingGame?: GameInfo | null;
  favoriteGameIdSet: Set<string>;
  setOptionsEntries: (entries: OptionEntry[]) => void;
  setOptionsFocusIndex: (index: number) => void;
  setOptionsOpen: (open: boolean) => void;
  playUiSound: (kind: SoundKind) => void;
  spotlightEntryHasGame: (entry: SpotlightEntry | undefined) => entry is { kind: "recent"; game: GameInfo };
  /** When the in-app video player is open, options apply to this file path. */
  localVideoFilePathForOptions: string | null;
  bumpMediaListRefresh: () => void;
  closeLocalVideoPlayer: () => void;
  setSelectedMediaIndex: (updater: (prev: number) => number) => void;
}

export interface SettingsActivateContext {
  settingsSubcategory: SettingsSubcategory;
  selectedSettingIndex: number;
  displayItems: DisplayItem[];
  currentStreamingGame?: GameInfo | null;
  onExitApp?: () => void;
  onExitControllerMode?: () => void;
  onSettingChange?: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  settings: ControllerLibrarySettings;
  setLastRootSettingIndex: (index: number) => void;
  setLastSystemMenuIndex: (index: number) => void;
  setLastThemeRootIndex: (index: number) => void;
  setSettingsSubcategory: (value: SettingsSubcategory) => void;
  setSelectedSettingIndex: (index: number) => void;
  setEditingThemeChannel: (value: null | "r" | "g" | "b") => void;
  setCategoryIndex: (updater: (prev: number) => number) => void;
  playUiSound: (kind: SoundKind) => void;
  sanitizeControllerThemeStyle: (value: ControllerThemeStyle | undefined) => ControllerThemeStyle;
  themeStyleOrder: readonly ControllerThemeStyle[];
}

export interface SettingsSecondaryContext {
  settingsSubcategory: SettingsSubcategory;
  displayItems: DisplayItem[];
  selectedSettingIndex: number;
  onSettingChange?: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  settings: ControllerLibrarySettings;
  microphoneDevices: Array<{ deviceId: string; label: string }>;
  aspectRatioOptions: string[];
  resolutionOptions: string[];
  fpsOptions: number[];
  codecOptions: string[];
  setEditingThemeChannel: (value: null | "r" | "g" | "b") => void;
  setEditingBandwidth: (value: boolean) => void;
  playUiSound: (kind: SoundKind) => void;
}

export interface SettingsCancelContext {
  settingsSubcategory: SettingsSubcategory;
  editingBandwidth: boolean;
  editingThemeChannel: null | "r" | "g" | "b";
  lastThemeRootIndex: number;
  lastSystemMenuIndex: number;
  lastRootSettingIndex: number;
  setEditingBandwidth: (value: boolean) => void;
  setEditingThemeChannel: (value: null | "r" | "g" | "b") => void;
  setSettingsSubcategory: (value: SettingsSubcategory) => void;
  setSelectedSettingIndex: (index: number) => void;
  playUiSound: (kind: SoundKind) => void;
}

export interface MediaActivateContext {
  mediaSubcategory: MediaSubcategory;
  displayItems: DisplayItem[];
  selectedMediaIndex: number;
  setLastRootMediaIndex: (index: number) => void;
  setMediaSubcategory: (subcategory: MediaSubcategory) => void;
  setSelectedMediaIndex: (index: number) => void;
  mediaAssetItems: MediaListingEntry[];
  playUiSound: (kind: SoundKind) => void;
  /** In-app playback for Media > Videos (orchestrated outside this module). */
  openLocalVideoPlayer: (entry: MediaListingEntry) => void;
}

export interface MediaCancelContext {
  lastRootMediaIndex: number;
  setMediaSubcategory: (subcategory: MediaSubcategory) => void;
  setSelectedMediaIndex: (index: number) => void;
  playUiSound: (kind: SoundKind) => void;
}

export interface AllActivateContext {
  gameSubcategory: GameSubcategory;
  gamesRootPlane: "spotlight" | "categories";
  spotlightEntries: SpotlightEntry[];
  spotlightIndex: number;
  onResumeCloudSession?: () => void;
  selectedGameSubcategoryIndex: number;
  displayItems: DisplayItem[];
  selectedGame: GameInfo | null;
  selectedGameId: string;
  setLastRootGameIndex: (index: number) => void;
  setGameSubcategory: (subcategory: GameSubcategory) => void;
  setSelectedGameSubcategoryIndex: (index: number) => void;
  setGamesHubOpen: (open: boolean) => void;
  setGamesHubFocusIndex: (index: number) => void;
  setPs5Row: (row: "top" | "main" | "detail") => void;
  throttledOnSelectGame: (id: string) => void;
  gamesHubReturnSnapshotRef: React.MutableRefObject<GamesHubReturnSnapshot | null>;
  playUiSound: (kind: SoundKind) => void;
  spotlightEntryHasGame: (entry: SpotlightEntry | undefined) => entry is { kind: "recent"; game: GameInfo };
}

export interface AllSecondaryContext {
  gamesShelfBrowseActive: boolean;
  gameSubcategory: GameSubcategory;
  setLibrarySortId: (updater: (prev: LibrarySortId) => LibrarySortId) => void;
  playUiSound: (kind: SoundKind) => void;
}

export interface AllCancelContext {
  gamesHubOpen: boolean;
  gameSubcategory: GameSubcategory;
  lastRootGameIndex: number;
  gamesHubReturnSnapshotRef: React.MutableRefObject<GamesHubReturnSnapshot | null>;
  setGamesHubFocusIndex: (index: number) => void;
  setGamesHubOpen: (open: boolean) => void;
  setGameSubcategory: (subcategory: GameSubcategory) => void;
  setSelectedGameSubcategoryIndex: (index: number) => void;
  setGamesRootPlane: (plane: "spotlight" | "categories") => void;
  setSpotlightIndex: (index: number) => void;
  throttledOnSelectGame: (id: string) => void;
  playUiSound: (kind: SoundKind) => void;
  setCategoryIndex?: (index: number) => void;
  setHomeRootPlane?: (plane: HomeRootPlane) => void;
}

export type ControllerLibrarySettings = {
  resolution?: string;
  fps?: number;
  codec?: string;
  enableL4S?: boolean;
  enableCloudGsync?: boolean;
  microphoneDeviceId?: string;
  controllerUiSounds?: boolean;
  controllerBackgroundAnimations?: boolean;
  autoLoadControllerLibrary?: boolean;
  autoFullScreen?: boolean;
  aspectRatio?: string;
  maxBitrateMbps?: number;
  controllerThemeStyle?: ControllerThemeStyle;
  controllerThemeColor?: { r: number; g: number; b: number };
  controllerLibraryGameBackdrop?: boolean;
};
