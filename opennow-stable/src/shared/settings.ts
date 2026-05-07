import type { AndroidTouchPlacement, AndroidTouchSettings, Settings } from "./gfn";
import { DEFAULT_KEYBOARD_LAYOUT, getDefaultStreamPreferences } from "./gfn";

const DEFAULT_STREAM_PREFERENCES = getDefaultStreamPreferences();
const ANDROID_TOUCH_PLACEMENTS: readonly AndroidTouchPlacement[] = ["default", "compact", "lower", "split"];

export const LEGACY_ANDROID_TOUCH_SETTINGS_KEY = "opennow.android.touchControls.v1";
export const DEFAULT_ANDROID_TOUCH_SETTINGS: AndroidTouchSettings = {
  enabled: false,
  size: 1,
  opacity: 0.74,
  placement: "default",
  mousePad: true,
  mouseCapture: true,
};

function clampNumber(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) {
    return min;
  }
  return Math.max(min, Math.min(max, value));
}

export function normalizeAndroidTouchSettings(value: unknown): AndroidTouchSettings {
  const parsed = value && typeof value === "object" ? value as Partial<AndroidTouchSettings> : {};
  const placement = ANDROID_TOUCH_PLACEMENTS.includes(parsed.placement as AndroidTouchPlacement)
    ? parsed.placement as AndroidTouchPlacement
    : DEFAULT_ANDROID_TOUCH_SETTINGS.placement;

  return {
    enabled: typeof parsed.enabled === "boolean" ? parsed.enabled : DEFAULT_ANDROID_TOUCH_SETTINGS.enabled,
    size: clampNumber(Number(parsed.size ?? DEFAULT_ANDROID_TOUCH_SETTINGS.size), 0.72, 1.35),
    opacity: clampNumber(Number(parsed.opacity ?? DEFAULT_ANDROID_TOUCH_SETTINGS.opacity), 0.25, 1),
    placement,
    mousePad: typeof parsed.mousePad === "boolean" ? parsed.mousePad : DEFAULT_ANDROID_TOUCH_SETTINGS.mousePad,
    mouseCapture: typeof parsed.mouseCapture === "boolean" ? parsed.mouseCapture : DEFAULT_ANDROID_TOUCH_SETTINGS.mouseCapture,
  };
}

export function normalizeSettings(value: Partial<Settings> = {}): Settings {
  return {
    ...DEFAULT_SETTINGS,
    ...value,
    androidTouchControls: normalizeAndroidTouchSettings(value.androidTouchControls),
  };
}

export const DEFAULT_SETTINGS: Settings = {
  resolution: "1920x1080",
  aspectRatio: "16:9",
  posterSizeScale: 1,
  fps: 60,
  maxBitrateMbps: 75,
  codec: DEFAULT_STREAM_PREFERENCES.codec,
  decoderPreference: "auto",
  encoderPreference: "auto",
  colorQuality: DEFAULT_STREAM_PREFERENCES.colorQuality,
  region: "",
  clipboardPaste: false,
  mouseSensitivity: 1,
  mouseAcceleration: 1,
  shortcutToggleStats: "F3",
  shortcutTogglePointerLock: "F8",
  shortcutToggleFullscreen: "F10",
  shortcutStopStream: "Ctrl+Shift+Q",
  shortcutToggleAntiAfk: "Ctrl+Shift+K",
  shortcutToggleMicrophone: "Ctrl+Shift+M",
  shortcutScreenshot: "F11",
  shortcutToggleRecording: "F12",
  microphoneMode: "disabled",
  microphoneDeviceId: "",
  hideStreamButtons: false,
  showAntiAfkIndicator: true,
  showStatsOnLaunch: false,
  controllerMode: false,
  controllerUiSounds: false,
  controllerBackgroundAnimations: false,
  autoLoadControllerLibrary: false,
  autoFullScreen: false,
  favoriteGameIds: [],
  sessionCounterEnabled: false,
  sessionClockShowEveryMinutes: 60,
  sessionClockShowDurationSeconds: 30,
  windowWidth: 1400,
  windowHeight: 900,
  keyboardLayout: DEFAULT_KEYBOARD_LAYOUT,
  gameLanguage: "en_US",
  enableL4S: false,
  enableCloudGsync: false,
  discordRichPresence: false,
  autoCheckForUpdates: true,
  androidTouchControls: DEFAULT_ANDROID_TOUCH_SETTINGS,
};
