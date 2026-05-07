import type { GameInfo, MediaListingEntry, Settings, ControllerThemeStyle } from "@shared/gfn";
import type { PlaytimeStore } from "../../../utils/usePlaytime";

export interface ControllerLibraryPageProps {
  games: GameInfo[];
  isLoading: boolean;
  selectedGameId: string;
  uiSoundsEnabled: boolean;
  selectedVariantByGameId: Record<string, string>;
  favoriteGameIds: string[];
  userName?: string;
  userAvatarUrl?: string;
  subscriptionInfo: import("@shared/gfn").SubscriptionInfo | null;
  playtimeData?: PlaytimeStore;
  onSelectGame: (id: string) => void;
  onSelectGameVariant: (gameId: string, variantId: string) => void;
  onToggleFavoriteGame: (gameId: string) => void;
  onPlayGame: (game: GameInfo) => void;
  onOpenSettings?: () => void;
  currentStreamingGame?: GameInfo | null;
  onResumeGame?: (game: GameInfo) => void;
  onCloseGame?: () => void;
  onExitApp?: () => void;
  settings?: {
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
    posterSizeScale?: number;
    maxBitrateMbps?: number;
    controllerThemeStyle?: ControllerThemeStyle;
    controllerThemeColor?: { r: number; g: number; b: number };
    controllerLibraryGameBackdrop?: boolean;
  };
  resolutionOptions?: string[];
  fpsOptions?: number[];
  codecOptions?: string[];
  aspectRatioOptions?: string[];
  onSettingChange?: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  onExitControllerMode?: () => void;
  sessionStartedAtMs?: number | null;
  isStreaming?: boolean;
  inStreamMenu?: boolean;
  streamMenuVolume?: number;
  onStreamMenuVolumeChange?: (volume01: number) => void;
  streamMenuMicLevel?: number;
  onStreamMenuMicLevelChange?: (level01: number) => void;
  /** Live mic track from the streaming client; used for the in-stream mic level test meter. */
  streamMicTrack?: MediaStreamTrack | null;
  onStreamMenuToggleMicrophone?: () => void;
  onStreamMenuToggleFullscreen?: () => void;
  streamMenuMicOn?: boolean;
  streamMenuIsFullscreen?: boolean;
  cloudSessionResumable?: boolean;
  cloudResumeTitle?: string | null;
  cloudResumeCoverUrl?: string | null;
  onResumeCloudSession?: () => void;
  cloudResumeBusy?: boolean;
}

export type Direction = "up" | "down" | "left" | "right";
export type TopCategory = "current" | "all" | "settings" | "media";
export type SoundKind = "move" | "confirm";
export type SettingsSubcategory = "root" | "Network" | "Audio" | "Video" | "System" | "Theme" | "ThemeColor" | "ThemeStyle";
export type MediaSubcategory = "root" | "Videos" | "Screenshots";
export type GameSubcategory = "root" | "all" | "favorites" | `genre:${string}`;
export type LibrarySortId = "recent" | "az" | "za" | "favoritesFirst";

export type HomeRootPlane = "spotlight" | "actions";

export type GamesHubReturnSnapshot = {
  gameSubcategory: GameSubcategory;
  selectedGameSubcategoryIndex: number;
  gamesRootPlane: "spotlight" | "categories";
  spotlightIndex: number;
  restoreSelectedGameId?: string;
  /** When hub was opened from Home spotlight, restore this top tab on back. */
  restoreCategoryIndex?: number;
  restoreHomeRootPlane?: HomeRootPlane;
};

export type SpotlightEntry =
  | { kind: "cloudResume"; title: string; coverUrl: string | null; busy: boolean }
  | { kind: "recent"; game: GameInfo | null };

export type PlaceholderTemplate = { title: string; subtitle: string };
export type ThemeRgb = { r: number; g: number; b: number };
export type MediaHubSlot =
  | { kind: "asset"; item: MediaListingEntry }
  | { kind: "placeholder"; id: string; title: string; subtitle: string };
