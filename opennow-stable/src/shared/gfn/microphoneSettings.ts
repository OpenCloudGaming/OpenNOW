import type { VideoCodec, VideoAccelerationPreference, ColorQuality, GameLanguage, KeyboardLayout } from "./streamPreferences";
import { normalizeStreamPreferences } from "./streamPreferences";


export type MicrophoneMode = "disabled" | "push-to-talk" | "voice-activity";
export type AspectRatio = "16:9" | "16:10" | "21:9" | "32:9";
export type RuntimePlatform =
  | "aix"
  | "android"
  | "cygwin"
  | "darwin"
  | "freebsd"
  | "haiku"
  | "linux"
  | "netbsd"
  | "openbsd"
  | "sunos"
  | "win32"
  | "unknown";

export type MacOsMicrophoneAccessStatus = "not-determined" | "granted" | "denied" | "restricted" | "unknown";

export interface MicrophonePermissionResult {
  platform: RuntimePlatform;
  isMacOs: boolean;
  status: MacOsMicrophoneAccessStatus | "not-applicable";
  granted: boolean;
  canRequest: boolean;
  shouldUseBrowserApi: boolean;
}

export interface Settings {
  resolution: string;
  aspectRatio: AspectRatio;
  posterSizeScale: number;
  fps: number;
  maxBitrateMbps: number;
  codec: VideoCodec;
  decoderPreference: VideoAccelerationPreference;
  encoderPreference: VideoAccelerationPreference;
  colorQuality: ColorQuality;
  region: string;
  clipboardPaste: boolean;
  mouseSensitivity: number;
  mouseAcceleration: number;
  shortcutToggleStats: string;
  shortcutTogglePointerLock: string;
  shortcutToggleFullscreen: string;
  shortcutStopStream: string;
  shortcutToggleAntiAfk: string;
  shortcutToggleMicrophone: string;
  shortcutScreenshot: string;
  shortcutToggleRecording: string;
  microphoneMode: MicrophoneMode;
  microphoneDeviceId: string;
  hideStreamButtons: boolean;
  showAntiAfkIndicator: boolean;
  showStatsOnLaunch: boolean;
  /** Skip the free-tier queue server selection modal and launch with default routing */
  hideServerSelector: boolean;
  controllerMode: boolean;
  controllerUiSounds: boolean;
  autoLoadControllerLibrary: boolean;
  /** When true, controller-mode overlays will show animated background orbs */
  controllerBackgroundAnimations: boolean;
  /** When true, the app will automatically enter fullscreen when controller mode triggers it */
  autoFullScreen: boolean;
  favoriteGameIds: string[];
  sessionCounterEnabled: boolean;
  sessionClockShowEveryMinutes: number;
  sessionClockShowDurationSeconds: number;
  windowWidth: number;
  windowHeight: number;
  /** Keyboard layout for mapping physical keys inside the remote session */
  keyboardLayout: KeyboardLayout;
  /** In-game language setting (sent to GFN servers via languageCode parameter) */
  gameLanguage: GameLanguage;
  /** Experimental request for Low Latency, Low Loss, Scalable throughput on new sessions */
  enableL4S: boolean;
  /** Request Cloud G-Sync / Variable Refresh Rate on new sessions */
  enableCloudGsync: boolean;
  /** Show the currently streaming game as Discord Rich Presence activity */
  discordRichPresence: boolean;
  /** Automatically check GitHub Releases for app updates in the background */
  autoCheckForUpdates: boolean;
}

export const DEFAULT_STREAM_PREFERENCES: Readonly<Pick<Settings, "codec" | "colorQuality">> = Object.freeze({
  codec: "H264",
  colorQuality: "10bit_420",
});

export function getDefaultStreamPreferences(): Pick<Settings, "codec" | "colorQuality"> {
  const normalized = normalizeStreamPreferences(
    DEFAULT_STREAM_PREFERENCES.codec,
    DEFAULT_STREAM_PREFERENCES.colorQuality,
  );
  return {
    codec: normalized.codec,
    colorQuality: normalized.colorQuality,
  };
}

