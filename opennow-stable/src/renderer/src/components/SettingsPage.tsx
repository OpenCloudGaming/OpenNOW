import { Globe, Save, Check, Search, X, Loader, Zap, Mic, FileDown } from "lucide-react";
import { Monitor, Volume2, Mouse, Keyboard, Settings2, Globe, Save, Check, Search, X, Loader, Cpu, Zap, MessageSquare, Joystick, Sun, RefreshCw, RotateCcw, Mic, MicOff } from "lucide-react";import { useState, useCallback, useMemo, useEffect, useRef } from "react";
import { Monitor, Volume2, Mouse, Settings2, Globe, Save, Check, Search, X, Loader, Cpu, Zap, MessageSquare, Joystick, Sun, RefreshCw, RotateCcw, Mic, MicOff } from "lucide-react";
import { useState, useCallback, useMemo, useEffect, useRef } from "react";import type { JSX } from "react";

import type {
  Settings,
  StreamRegion,
  VideoCodec,
  ColorQuality,
  EntitledResolution,
  VideoAccelerationPreference,
  MicrophoneMode,
  HdrStreamingMode,
  HdrCapability,
  MicMode,
  MicDeviceInfo,
  PlatformInfo,
  VideoDecodeBackend,
  KeyboardLayout,} from "@shared/gfn";
} from "@shared/gfn";import { colorQualityRequiresHevc } from "@shared/gfn";
import { formatShortcutForDisplay, normalizeShortcut } from "../shortcuts";

interface SettingsPageProps {
  settings: Settings;
  regions: StreamRegion[];
  onSettingChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

const codecOptions: VideoCodec[] = ["H264", "H265", "AV1"];

const accelerationOptions: { value: VideoAccelerationPreference; label: string }[] = [
  { value: "auto", label: "Auto" },
  { value: "hardware", label: "Hardware" },
  { value: "software", label: "Software (CPU)" },
];

const colorQualityOptions: { value: ColorQuality; label: string; description: string }[] = [
  { value: "8bit_420", label: "8-bit 4:2:0", description: "Most compatible" },
  { value: "8bit_444", label: "8-bit 4:4:4", description: "Better color" },
  { value: "10bit_420", label: "10-bit 4:2:0", description: "HDR ready" },
  { value: "10bit_444", label: "10-bit 4:4:4", description: "Best quality" },
];

/* ── Static fallbacks (used when MES API is unavailable) ─────────── */

interface ResolutionPreset {
  value: string;
  label: string;
}

interface FpsPreset {
  value: number;
}

const STATIC_RESOLUTION_PRESETS: ResolutionPreset[] = [
  { value: "1280x720", label: "720p" },
  { value: "1920x1080", label: "1080p" },
  { value: "2560x1440", label: "1440p" },
  { value: "3840x2160", label: "4K" },
  { value: "2560x1080", label: "Ultrawide 1080p" },
  { value: "3440x1440", label: "Ultrawide 1440p" },
  { value: "5120x1440", label: "Super Ultrawide" },
];

const STATIC_FPS_PRESETS: FpsPreset[] = [
  { value: 30 },
  { value: 60 },
  { value: 90 },
  { value: 120 },
  { value: 144 },
  { value: 165 },
  { value: 240 },
  { value: 360 },
];

const isMac = navigator.platform.toLowerCase().includes("mac");
const shortcutExamples = "Examples: F3, Ctrl+Shift+Q, Ctrl+Shift+K";
const shortcutDefaults = {
  shortcutToggleStats: "F3",
  shortcutTogglePointerLock: "F8",
  shortcutStopStream: "Ctrl+Shift+Q",
  shortcutToggleAntiAfk: "Ctrl+Shift+K",
  shortcutToggleMicrophone: "Ctrl+Shift+M",
} as const;

const microphoneModeOptions: Array<{ value: MicrophoneMode; label: string }> = [
  { value: "disabled", label: "Disabled" },
  { value: "push-to-talk", label: "Push-to-Talk" },
  { value: "voice-activity", label: "Voice Activity" },
];

/* ── Aspect ratio helpers ─────────────────────────────────────────── */

const ASPECT_RATIO_ORDER = [
  "16:9 Standard",
  "16:10 Widescreen",
  "21:9 Ultrawide",
  "32:9 Super Ultrawide",
  "4:3 Legacy",
  "Other",
] as const;

function classifyAspectRatio(width: number, height: number): string {
  const ratio = width / height;
  if (Math.abs(ratio - 16 / 9) < 0.05) return "16:9 Standard";
  if (Math.abs(ratio - 16 / 10) < 0.05) return "16:10 Widescreen";
  if (Math.abs(ratio - 21 / 9) < 0.05) return "21:9 Ultrawide";
  if (Math.abs(ratio - 32 / 9) < 0.05) return "32:9 Super Ultrawide";
  if (Math.abs(ratio - 4 / 3) < 0.05) return "4:3 Legacy";
  return "Other";
}

function friendlyResolutionName(width: number, height: number): string {
  if (width === 1280 && height === 720) return "720p (HD)";
  if (width === 1920 && height === 1080) return "1080p (FHD)";
  if (width === 2560 && height === 1440) return "1440p (QHD)";
  if (width === 3840 && height === 2160) return "4K (UHD)";
  if (width === 2560 && height === 1080) return "2560x1080 (UW)";
  if (width === 3440 && height === 1440) return "3440x1440 (UW)";
  if (width === 5120 && height === 1440) return "5120x1440 (SUW)";
  return `${width}x${height}`;
}

interface ResolutionGroup {
  category: string;
  resolutions: { width: number; height: number; value: string; label: string }[];
}

function groupResolutions(entitled: EntitledResolution[]): ResolutionGroup[] {
  // Deduplicate by (width, height)
  const seen = new Set<string>();
  const unique: { width: number; height: number }[] = [];
  // Sort by width desc, height desc
  const sorted = [...entitled].sort((a, b) => b.width - a.width || b.height - a.height);
  for (const res of sorted) {
    const key = `${res.width}x${res.height}`;
    if (seen.has(key)) continue;
    seen.add(key);
    unique.push(res);
  }

  // Group by aspect ratio
  const groupMap = new Map<string, { width: number; height: number; value: string; label: string }[]>();
  for (const res of unique) {
    const cat = classifyAspectRatio(res.width, res.height);
    const value = `${res.width}x${res.height}`;
    const label = friendlyResolutionName(res.width, res.height);
    if (!groupMap.has(cat)) groupMap.set(cat, []);
    groupMap.get(cat)!.push({ width: res.width, height: res.height, value, label });
  }

  // Return in canonical order
  const result: ResolutionGroup[] = [];
  for (const cat of ASPECT_RATIO_ORDER) {
    const items = groupMap.get(cat);
    if (items && items.length > 0) {
      result.push({ category: cat, resolutions: items });
    }
  }
  return result;
}

function getFpsForResolution(entitled: EntitledResolution[], resolution: string): number[] {
  const parts = resolution.split("x");
  const w = parseInt(parts[0], 10);
  const h = parseInt(parts[1], 10);

  let fpsList = entitled
    .filter((r) => r.width === w && r.height === h)
    .map((r) => r.fps);

  // Fallback: if no exact match, collect all FPS from all resolutions
  if (fpsList.length === 0) {
    fpsList = entitled.map((r) => r.fps);
  }

  // Deduplicate and sort ascending
  return [...new Set(fpsList)].sort((a, b) => a - b);
}

/* ── Codec diagnostics ────────────────────────────────────────────── */

interface CodecTestResult {
  codec: string;
  /** Whether WebRTC can negotiate this codec at all */
  webrtcSupported: boolean;
  /** Whether MediaCapabilities reports decode support */
  decodeSupported: boolean;
  /** Whether MediaCapabilities says HW-accelerated (powerEfficient) */
  hwAccelerated: boolean;
  /** Whether encode is supported */
  encodeSupported: boolean;
  /** Whether encode is HW-accelerated */
  encodeHwAccelerated: boolean;
  /** Human-readable decode method (e.g. "D3D11", "VAAPI", "VideoToolbox", "Software") */
  decodeVia: string;
  /** Human-readable encode method */
  encodeVia: string;
  /** Profiles found in WebRTC capabilities */
  profiles: string[];
}

/** Map of codec name to MediaCapabilities contentType and profile strings */
const CODEC_TEST_CONFIGS: {
  name: string;
  webrtcMime: string;
  decodeContentType: string;
  encodeContentType: string;
  profiles: { label: string; contentType: string }[];
}[] = [
  {
    name: "H264",
    webrtcMime: "video/H264",
    decodeContentType: "video/mp4; codecs=\"avc1.42E01E\"",
    encodeContentType: "video/mp4; codecs=\"avc1.42E01E\"",
    profiles: [
      { label: "Baseline", contentType: "video/mp4; codecs=\"avc1.42E01E\"" },
      { label: "Main", contentType: "video/mp4; codecs=\"avc1.4D401E\"" },
      { label: "High", contentType: "video/mp4; codecs=\"avc1.64001E\"" },
    ],
  },
  {
    name: "H265",
    webrtcMime: "video/H265",
    decodeContentType: "video/mp4; codecs=\"hev1.1.6.L93.B0\"",
    encodeContentType: "video/mp4; codecs=\"hev1.1.6.L93.B0\"",
    profiles: [
      { label: "Main", contentType: "video/mp4; codecs=\"hev1.1.6.L93.B0\"" },
      { label: "Main 10", contentType: "video/mp4; codecs=\"hev1.2.4.L93.B0\"" },
    ],
  },
  {
    name: "AV1",
    webrtcMime: "video/AV1",
    decodeContentType: "video/mp4; codecs=\"av01.0.08M.08\"",
    encodeContentType: "video/mp4; codecs=\"av01.0.08M.08\"",
    profiles: [
      { label: "Main 8-bit", contentType: "video/mp4; codecs=\"av01.0.08M.08\"" },
      { label: "Main 10-bit", contentType: "video/mp4; codecs=\"av01.0.08M.10\"" },
    ],
  },
];

const CODEC_TEST_RESULTS_STORAGE_KEY = "opennow.codec-test-results.v1";
const ENTITLED_RESOLUTIONS_STORAGE_KEY = "opennow.entitled-resolutions.v1";

interface EntitledResolutionsCache {
  userId: string;
  entitledResolutions: EntitledResolution[];
}

function loadStoredCodecResults(): CodecTestResult[] | null {
  try {
    const raw = window.sessionStorage.getItem(CODEC_TEST_RESULTS_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return null;
    return parsed as CodecTestResult[];
  } catch {
    return null;
  }
}

function loadCachedEntitledResolutions(): EntitledResolutionsCache | null {
  try {
    const raw = window.sessionStorage.getItem(ENTITLED_RESOLUTIONS_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<EntitledResolutionsCache>;
    if (!parsed || typeof parsed.userId !== "string" || !Array.isArray(parsed.entitledResolutions)) {
      return null;
    }
    return {
      userId: parsed.userId,
      entitledResolutions: parsed.entitledResolutions,
    };
  } catch {
    return null;
  }
}

function saveCachedEntitledResolutions(cache: EntitledResolutionsCache): void {
  try {
    window.sessionStorage.setItem(ENTITLED_RESOLUTIONS_STORAGE_KEY, JSON.stringify(cache));
  } catch {
    // Ignore storage failures
  }
}

function isLinuxArmClient(): boolean {
  const platform = navigator.platform?.toLowerCase() ?? "";
  const ua = navigator.userAgent?.toLowerCase() ?? "";
  const linux = platform.includes("linux") || ua.includes("linux");
  const arm = /(aarch64|arm64|armv\d|arm)/.test(platform) || /(aarch64|arm64|armv\d|arm)/.test(ua);
  return linux && arm;
}

function guessDecodeBackend(hwAccelerated: boolean): string {
  if (!hwAccelerated) return "Software (CPU)";
  const platform = navigator.platform?.toLowerCase() ?? "";
  const ua = navigator.userAgent?.toLowerCase() ?? "";
  if (platform.includes("win") || ua.includes("windows")) return "D3D11 (GPU)";
  if (platform.includes("mac") || ua.includes("macintosh")) return "VideoToolbox (GPU)";
  if (platform.includes("linux") || ua.includes("linux")) {
    return isLinuxArmClient() ? "V4L2 (GPU)" : "VA-API (GPU)";
  }
  return "Hardware (GPU)";
}

function guessEncodeBackend(hwAccelerated: boolean): string {
  if (!hwAccelerated) return "Software (CPU)";
  const platform = navigator.platform?.toLowerCase() ?? "";
  const ua = navigator.userAgent?.toLowerCase() ?? "";
  if (platform.includes("win") || ua.includes("windows")) return "Media Foundation (GPU)";
  if (platform.includes("mac") || ua.includes("macintosh")) return "VideoToolbox (GPU)";
  if (platform.includes("linux") || ua.includes("linux")) {
    return isLinuxArmClient() ? "V4L2 (GPU)" : "VA-API (GPU)";
  }
  return "Hardware (GPU)";
}

async function testCodecSupport(): Promise<CodecTestResult[]> {
  const results: CodecTestResult[] = [];

  // Get WebRTC receiver capabilities once
  const webrtcCaps = RTCRtpReceiver.getCapabilities?.("video");
  const webrtcCodecMimes = new Set(
    webrtcCaps?.codecs.map((c) => c.mimeType.toLowerCase()) ?? [],
  );

  // Collect WebRTC profiles per codec
  const webrtcProfiles = new Map<string, string[]>();
  if (webrtcCaps) {
    for (const c of webrtcCaps.codecs) {
      const mime = c.mimeType.toLowerCase();
      const sdpLine = (c as unknown as Record<string, string>).sdpFmtpLine ?? "";
      if (!mime.includes("rtx") && !mime.includes("red") && !mime.includes("ulpfec")) {
        const existing = webrtcProfiles.get(mime) ?? [];
        if (sdpLine) existing.push(sdpLine);
        webrtcProfiles.set(mime, existing);
      }
    }
  }

  for (const config of CODEC_TEST_CONFIGS) {
    const webrtcSupported = webrtcCodecMimes.has(config.webrtcMime.toLowerCase());
    const profiles = webrtcProfiles.get(config.webrtcMime.toLowerCase()) ?? [];

    // Test decode via MediaCapabilities API
    let decodeSupported = false;
    let hwAccelerated = false;
    try {
      const decodeResult = await navigator.mediaCapabilities.decodingInfo({
        type: "webrtc",
        video: {
          contentType: config.webrtcMime === "video/H265" ? "video/h265" : config.webrtcMime.toLowerCase(),
          width: 1920,
          height: 1080,
          framerate: 60,
          bitrate: 20_000_000,
        },
      });
      decodeSupported = decodeResult.supported;
      hwAccelerated = decodeResult.powerEfficient;
    } catch {
      // webrtc type may not be supported, fall back to file type
      try {
        const decodeResult = await navigator.mediaCapabilities.decodingInfo({
          type: "file",
          video: {
            contentType: config.decodeContentType,
            width: 1920,
            height: 1080,
            framerate: 60,
            bitrate: 20_000_000,
          },
        });
        decodeSupported = decodeResult.supported;
        hwAccelerated = decodeResult.powerEfficient;
      } catch {
        // Codec not recognized at all
      }
    }

    // Test encode via MediaCapabilities API
    let encodeSupported = false;
    let encodeHwAccelerated = false;
    try {
      const encodeResult = await navigator.mediaCapabilities.encodingInfo({
        type: "webrtc",
        video: {
          contentType: config.webrtcMime === "video/H265" ? "video/h265" : config.webrtcMime.toLowerCase(),
          width: 1920,
          height: 1080,
          framerate: 60,
          bitrate: 20_000_000,
        },
      });
      encodeSupported = encodeResult.supported;
      encodeHwAccelerated = encodeResult.powerEfficient;
    } catch {
      try {
        const encodeResult = await navigator.mediaCapabilities.encodingInfo({
          type: "record",
          video: {
            contentType: config.encodeContentType,
            width: 1920,
            height: 1080,
            framerate: 60,
            bitrate: 20_000_000,
          },
        });
        encodeSupported = encodeResult.supported;
        encodeHwAccelerated = encodeResult.powerEfficient;
      } catch {
        // Codec not recognized at all
      }
    }

    results.push({
      codec: config.name,
      webrtcSupported,
      decodeSupported: decodeSupported || webrtcSupported, // WebRTC support implies decode
      hwAccelerated,
      encodeSupported,
      encodeHwAccelerated,
      decodeVia: (decodeSupported || webrtcSupported)
        ? guessDecodeBackend(hwAccelerated)
        : "Unsupported",
      encodeVia: encodeSupported
        ? guessEncodeBackend(encodeHwAccelerated)
        : "Unsupported",
      profiles,
    });
  }

  return results;
}

/* ── Component ────────────────────────────────────────────────────── */

export function SettingsPage({ settings, regions, onSettingChange }: SettingsPageProps): JSX.Element {
  const [savedIndicator, setSavedIndicator] = useState(false);
  const [regionSearch, setRegionSearch] = useState("");
  const [regionDropdownOpen, setRegionDropdownOpen] = useState(false);

  // Codec diagnostics
  const initialCodecResults = useMemo(() => loadStoredCodecResults(), []);
  const [codecResults, setCodecResults] = useState<CodecTestResult[] | null>(initialCodecResults);
  const [codecTesting, setCodecTesting] = useState(false);
  const [codecTestOpen, setCodecTestOpen] = useState(() => initialCodecResults !== null);
  const platformHardwareLabel = useMemo(() => {
    const platform = navigator.platform.toLowerCase();
    if (platform.includes("win")) return "D3D11 / DXVA";
    if (platform.includes("mac")) return "VideoToolbox";
    if (platform.includes("linux")) return isLinuxArmClient() ? "V4L2" : "VA-API";
    return "Hardware";
  }, []);

  const runCodecTest = useCallback(async () => {
    setCodecTesting(true);
    setCodecTestOpen(true);
    try {
      const results = await testCodecSupport();
      setCodecResults(results);
    } catch (err) {
      console.error("Codec test failed:", err);
    } finally {
      setCodecTesting(false);
    }
  }, []);

  useEffect(() => {
    try {
      if (codecResults && codecResults.length > 0) {
        window.sessionStorage.setItem(CODEC_TEST_RESULTS_STORAGE_KEY, JSON.stringify(codecResults));
      } else {
        window.sessionStorage.removeItem(CODEC_TEST_RESULTS_STORAGE_KEY);
      }
    } catch {
      // Ignore storage failures (private mode / denied storage)
    }
  }, [codecResults]);

  const [toggleStatsInput, setToggleStatsInput] = useState(settings.shortcutToggleStats);
  const [togglePointerLockInput, setTogglePointerLockInput] = useState(settings.shortcutTogglePointerLock);
  const [stopStreamInput, setStopStreamInput] = useState(settings.shortcutStopStream);
  const [toggleAntiAfkInput, setToggleAntiAfkInput] = useState(settings.shortcutToggleAntiAfk);
  const [toggleMicrophoneInput, setToggleMicrophoneInput] = useState(settings.shortcutToggleMicrophone);
  const [toggleStatsError, setToggleStatsError] = useState(false);
  const [togglePointerLockError, setTogglePointerLockError] = useState(false);
  const [stopStreamError, setStopStreamError] = useState(false);
  const [toggleAntiAfkError, setToggleAntiAfkError] = useState(false);
  const [toggleMicrophoneError, setToggleMicrophoneError] = useState(false);

  const [micDevices, setMicDevices] = useState<MicDeviceInfo[]>([]);
  const [micDevicesLoading, setMicDevicesLoading] = useState(false);
  const [micToggleShortcutInput, setMicToggleShortcutInput] = useState(settings.shortcutToggleMic);
  const [micToggleShortcutError, setMicToggleShortcutError] = useState(false);
  const micLevelRef = useRef<HTMLDivElement>(null);
  const [micTestLevel, setMicTestLevel] = useState(0);
  const [micTestRunning, setMicTestRunning] = useState(false);
  const [micTestError, setMicTestError] = useState<string | null>(null);
  const micTestStreamRef = useRef<MediaStream | null>(null);
  const micTestContextRef = useRef<AudioContext | null>(null);
  const micTestTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const micTestAnalyserRef = useRef<AnalyserNode | null>(null);
  const micTestBufferRef = useRef<Float32Array<ArrayBuffer> | null>(null);

  const refreshMicDevices = useCallback(async () => {
    setMicDevicesLoading(true);
    try {
      const devices = await navigator.mediaDevices.enumerateDevices();
      const audioInputs = devices.filter((d) => d.kind === "audioinput" && d.deviceId !== "");
      const defaultDev = audioInputs.find((d) => d.deviceId === "default");
      const defaultGroupId = defaultDev?.groupId;
      const mapped: MicDeviceInfo[] = audioInputs.map((d) => ({
        deviceId: d.deviceId,
        label: d.label || `Microphone (${d.deviceId.slice(0, 8)})`,
        isDefault: d.deviceId === "default" || (!!defaultGroupId && d.groupId === defaultGroupId && d.deviceId !== "default"),
      }));
      setMicDevices(mapped);
    } catch {
      setMicDevices([]);
    } finally {
      setMicDevicesLoading(false);
    }
  }, []);

  useEffect(() => {
    void refreshMicDevices();
    const handler = (): void => { void refreshMicDevices(); };
    navigator.mediaDevices.addEventListener("devicechange", handler);
    return () => navigator.mediaDevices.removeEventListener("devicechange", handler);
  }, [refreshMicDevices]);

  useEffect(() => {
    setMicToggleShortcutInput(settings.shortcutToggleMic);
  }, [settings.shortcutToggleMic]);

  const startMicTest = useCallback(async () => {
    setMicTestError(null);
    try {
      const constraints: MediaStreamConstraints = {
        audio: {
          deviceId: settings.micDeviceId ? { exact: settings.micDeviceId } : undefined,
          noiseSuppression: { ideal: settings.micNoiseSuppression },
          autoGainControl: { ideal: settings.micAutoGainControl },
          echoCancellation: { ideal: settings.micEchoCancellation },
        },
        video: false,
      };
      const stream = await navigator.mediaDevices.getUserMedia(constraints);
      micTestStreamRef.current = stream;

      const ctx = new AudioContext({ sampleRate: 48000, latencyHint: "interactive" });
      micTestContextRef.current = ctx;

      const src = ctx.createMediaStreamSource(stream);
      const gain = ctx.createGain();
      gain.gain.value = settings.micGain;
      const analyser = ctx.createAnalyser();
      analyser.fftSize = 256;
      analyser.smoothingTimeConstant = 0.3;
      micTestAnalyserRef.current = analyser;
      micTestBufferRef.current = new Float32Array(analyser.fftSize) as Float32Array<ArrayBuffer>;

      src.connect(gain);
      gain.connect(analyser);

      micTestTimerRef.current = setInterval(() => {
        if (!micTestAnalyserRef.current || !micTestBufferRef.current) return;
        micTestAnalyserRef.current.getFloatTimeDomainData(micTestBufferRef.current);
        let sum = 0;
        for (let i = 0; i < micTestBufferRef.current.length; i++) {
          const v = micTestBufferRef.current[i];
          sum += v * v;
        }
        const rms = Math.sqrt(sum / micTestBufferRef.current.length);
        const db = 20 * Math.log10(Math.max(rms, 1e-10));
        setMicTestLevel(Math.max(0, Math.min(1, (db + 60) / 60)));
      }, 50);

      setMicTestRunning(true);
    } catch (err: unknown) {
      setMicTestLevel(0);
      setMicTestRunning(false);
      if (err instanceof DOMException && (err.name === "NotAllowedError" || err.name === "PermissionDeniedError")) {
        setMicTestError("Microphone permission denied");
      } else if (err instanceof DOMException && (err.name === "NotFoundError" || err.name === "OverconstrainedError")) {
        setMicTestError("No microphone found");
      } else {
        setMicTestError("Failed to access microphone");
      }
    }
  }, [settings.micDeviceId, settings.micGain, settings.micNoiseSuppression, settings.micAutoGainControl, settings.micEchoCancellation]);

  const stopMicTest = useCallback(() => {
    if (micTestTimerRef.current) {
      clearInterval(micTestTimerRef.current);
      micTestTimerRef.current = null;
    }
    if (micTestContextRef.current) {
      void micTestContextRef.current.close().catch(() => {});
      micTestContextRef.current = null;
    }
    if (micTestStreamRef.current) {
      for (const track of micTestStreamRef.current.getTracks()) track.stop();
      micTestStreamRef.current = null;
    }
    micTestAnalyserRef.current = null;
    micTestBufferRef.current = null;
    setMicTestLevel(0);
    setMicTestRunning(false);
  }, []);

  useEffect(() => {
    return () => stopMicTest();
  }, [stopMicTest]);
  // Dynamic entitled resolutions from MES API
  const [entitledResolutions, setEntitledResolutions] = useState<EntitledResolution[]>([]);
  const [subscriptionLoading, setSubscriptionLoading] = useState(true);

  useEffect(() => {
    setToggleStatsInput(settings.shortcutToggleStats);
  }, [settings.shortcutToggleStats]);

  useEffect(() => {
    setTogglePointerLockInput(settings.shortcutTogglePointerLock);
  }, [settings.shortcutTogglePointerLock]);

  useEffect(() => {
    setStopStreamInput(settings.shortcutStopStream);
  }, [settings.shortcutStopStream]);

  useEffect(() => {
    setToggleAntiAfkInput(settings.shortcutToggleAntiAfk);
  }, [settings.shortcutToggleAntiAfk]);

  useEffect(() => {
    setToggleMicrophoneInput(settings.shortcutToggleMicrophone);
  }, [settings.shortcutToggleMicrophone]);

  // Fetch subscription data (cached per account; reload only when account changes)
  useEffect(() => {
    let cancelled = false;

    async function load(): Promise<void> {
      try {
        const sessionResult = await window.openNow.getAuthSession();
        const session = sessionResult.session;
        if (!session || cancelled) {
          setEntitledResolutions([]);
          setSubscriptionLoading(false);
          return;
        }

        const userId = session.user.userId;
        const cached = loadCachedEntitledResolutions();
        if (cached && cached.userId === userId) {
          setEntitledResolutions(cached.entitledResolutions);
          setSubscriptionLoading(false);
          return;
        }

        const sub = await window.openNow.fetchSubscription({
          userId,
        });

        if (!cancelled) {
          setEntitledResolutions(sub.entitledResolutions);
          saveCachedEntitledResolutions({
            userId,
            entitledResolutions: sub.entitledResolutions,
          });
        }
      } catch (err) {
        console.warn("Failed to fetch subscription for settings:", err);
      } finally {
        if (!cancelled) setSubscriptionLoading(false);
      }
    }

    load();
    return () => { cancelled = true; };
  }, []);

  const hasDynamic = entitledResolutions.length > 0;

  // Grouped resolution presets (dynamic)
  const resolutionGroups = useMemo(
    () => (hasDynamic ? groupResolutions(entitledResolutions) : []),
    [entitledResolutions, hasDynamic]
  );

  // Dynamic FPS presets based on current resolution
  const dynamicFpsOptions = useMemo(
    () => (hasDynamic ? getFpsForResolution(entitledResolutions, settings.resolution) : []),
    [entitledResolutions, settings.resolution, hasDynamic]
  );

  const handleChange = useCallback(
    <K extends keyof Settings>(key: K, value: Settings[K]) => {
      onSettingChange(key, value);
      setSavedIndicator(true);
      setTimeout(() => setSavedIndicator(false), 1500);
    },
    [onSettingChange]
  );

  /** Change color quality, auto-switching codec to H265 if the mode requires HEVC */
  const handleColorQualityChange = useCallback(
    (cq: ColorQuality) => {
      handleChange("colorQuality", cq);
      if (colorQualityRequiresHevc(cq) && settings.codec === "H264") {
        handleChange("codec", "H265");
      }
    },
    [handleChange, settings.codec]
  );

  // Microphone devices
  const [microphoneDevices, setMicrophoneDevices] = useState<MediaDeviceInfo[]>([]);
  const [microphonePermissionError, setMicrophonePermissionError] = useState<string | null>(null);
  const [microphoneModeDropdownOpen, setMicrophoneModeDropdownOpen] = useState(false);
  const [microphoneDeviceDropdownOpen, setMicrophoneDeviceDropdownOpen] = useState(false);
  const microphoneModeDropdownRef = useRef<HTMLDivElement | null>(null);
  const microphoneDeviceDropdownRef = useRef<HTMLDivElement | null>(null);

  // Enumerate microphone devices when mic mode is enabled
  useEffect(() => {
    if (settings.microphoneMode === "disabled") {
      setMicrophoneDevices([]);
      return;
    }

    let cancelled = false;

    async function enumerateDevices(): Promise<void> {
      try {
        // Request permission first to get device labels
        const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
        stream.getTracks().forEach(t => t.stop()); // Release the stream immediately

        const devices = await navigator.mediaDevices.enumerateDevices();
        if (!cancelled) {
          const audioInputs = devices.filter(d => d.kind === "audioinput");
          setMicrophoneDevices(audioInputs);
          setMicrophonePermissionError(null);
        }
      } catch (err) {
        console.error("[SettingsPage] Failed to enumerate microphone devices:", err);
        if (!cancelled) {
          setMicrophonePermissionError("Microphone access denied. Please allow microphone permission in your system settings.");
          setMicrophoneDevices([]);
        }
      }
    }

    enumerateDevices();
    return () => { cancelled = true; };
  }, [settings.microphoneMode]);

  const filteredRegions = useMemo(() => {
    if (!regionSearch.trim()) return regions;
    const q = regionSearch.trim().toLowerCase();
    return regions.filter((r) => r.name.toLowerCase().includes(q));
  }, [regions, regionSearch]);

  const selectedRegionName = useMemo(() => {
    if (!settings.region) return "Auto (Best)";
    const found = regions.find((r) => r.url === settings.region);
    return found?.name ?? settings.region;
  }, [settings.region, regions]);

  const selectedMicrophoneModeName = useMemo(() => {
    return microphoneModeOptions.find((option) => option.value === settings.microphoneMode)?.label ?? "Disabled";
  }, [settings.microphoneMode]);

  const selectedMicrophoneDeviceName = useMemo(() => {
    if (!settings.microphoneDeviceId) return "Default Device";
    const found = microphoneDevices.find((device) => device.deviceId === settings.microphoneDeviceId);
    return found?.label || "Selected Device";
  }, [settings.microphoneDeviceId, microphoneDevices]);

  useEffect(() => {
    if (settings.microphoneMode === "disabled") {
      setMicrophoneDeviceDropdownOpen(false);
    }
  }, [settings.microphoneMode]);

  useEffect(() => {
    const handlePointerDown = (event: MouseEvent): void => {
      const target = event.target as Node;
      if (microphoneModeDropdownRef.current && !microphoneModeDropdownRef.current.contains(target)) {
        setMicrophoneModeDropdownOpen(false);
      }
      if (microphoneDeviceDropdownRef.current && !microphoneDeviceDropdownRef.current.contains(target)) {
        setMicrophoneDeviceDropdownOpen(false);
      }
    };

    document.addEventListener("mousedown", handlePointerDown);
    return () => document.removeEventListener("mousedown", handlePointerDown);
  }, []);

  const handleShortcutBlur = <K extends keyof Settings>(
    key: K,
    rawValue: string,
    setInput: (value: string) => void,
    setError: (value: boolean) => void
  ): void => {
    const normalized = normalizeShortcut(rawValue.trim());
    if (!normalized.valid) {
      setError(true);
      return;
    }
    setError(false);
    setInput(normalized.canonical);
    if (settings[key] !== normalized.canonical) {
      handleChange(key, normalized.canonical as Settings[K]);
    }
  };

  const handleShortcutKeyDown = (e: React.KeyboardEvent): void => {
    if (e.key === "Enter") {
      (e.target as HTMLInputElement).blur();
    }
  };

  const areShortcutsDefault = useMemo(
    () =>
      settings.shortcutToggleStats === shortcutDefaults.shortcutToggleStats
      && settings.shortcutTogglePointerLock === shortcutDefaults.shortcutTogglePointerLock
      && settings.shortcutStopStream === shortcutDefaults.shortcutStopStream
      && settings.shortcutToggleAntiAfk === shortcutDefaults.shortcutToggleAntiAfk
      && settings.shortcutToggleMicrophone === shortcutDefaults.shortcutToggleMicrophone,
    [
      settings.shortcutToggleStats,
      settings.shortcutTogglePointerLock,
      settings.shortcutStopStream,
      settings.shortcutToggleAntiAfk,
      settings.shortcutToggleMicrophone,
    ]
  );

  const handleResetShortcuts = useCallback(() => {
    setToggleStatsInput(shortcutDefaults.shortcutToggleStats);
    setTogglePointerLockInput(shortcutDefaults.shortcutTogglePointerLock);
    setStopStreamInput(shortcutDefaults.shortcutStopStream);
    setToggleAntiAfkInput(shortcutDefaults.shortcutToggleAntiAfk);
    setToggleMicrophoneInput(shortcutDefaults.shortcutToggleMicrophone);
    setToggleStatsError(false);
    setTogglePointerLockError(false);
    setStopStreamError(false);
    setToggleAntiAfkError(false);
    setToggleMicrophoneError(false);

    const shortcutKeys = [
      "shortcutToggleStats",
      "shortcutTogglePointerLock",
      "shortcutStopStream",
      "shortcutToggleAntiAfk",
      "shortcutToggleMicrophone",
    ] as const;

    for (const key of shortcutKeys) {
      const value = shortcutDefaults[key];
      if (settings[key] !== value) {
        handleChange(key, value);
      }
    }
  }, [handleChange, settings]);

  return (
    <div className="settings-page">
      <header className="settings-header">
        <h1>Settings</h1>
        <div className={`settings-saved ${savedIndicator ? "visible" : ""}`}>
          <Check size={14} />
          Saved
        </div>
      </header>

      <div className="settings-sections">
        {/* ── Video ──────────────────────────────────────── */}
        <section className="settings-section">
          <div className="settings-section-header">
            <h2>Video</h2>
          </div>

          <div className="settings-rows">
            {/* Resolution — dynamic or static chips */}
            <div className="settings-row settings-row--column">
              <label className="settings-label">
                Resolution
                {subscriptionLoading && <Loader size={12} className="settings-loading-icon" />}
              </label>

              {hasDynamic ? (
                <div className="settings-preset-groups">
                  {resolutionGroups.map((group) => (
                    <div key={group.category} className="settings-preset-group">
                      <span className="settings-preset-group-label">{group.category}</span>
                      <div className="settings-chip-row">
                        {group.resolutions.map((res) => (
                          <button
                            key={res.value}
                            className={`settings-chip ${settings.resolution === res.value ? "active" : ""}`}
                            onClick={() => {
                              handleChange("resolution", res.value);
                            }}
                          >
                            <span>{res.label}</span>
                          </button>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="settings-chip-row">
                  {STATIC_RESOLUTION_PRESETS.map((preset) => (
                    <button
                      key={preset.value}
                      className={`settings-chip ${settings.resolution === preset.value ? "active" : ""}`}
                      onClick={() => {
                        handleChange("resolution", preset.value);
                      }}
                    >
                      <span>{preset.label}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>

            {/* FPS — dynamic or static chips */}
            <div className="settings-row">
              <label className="settings-label">FPS</label>
              <div className="settings-chip-row">
                {(hasDynamic ? dynamicFpsOptions.map((v) => ({ value: v })) : STATIC_FPS_PRESETS).map(
                  (preset) => (
                    <button
                      key={preset.value}
                      className={`settings-chip ${settings.fps === preset.value ? "active" : ""}`}
                      onClick={() => {
                        handleChange("fps", preset.value);
                      }}
                    >
                      <span>{preset.value}</span>
                    </button>
                  )
                )}
              </div>
            </div>

            {/* Codec */}
            <div className="settings-row">
              <label className="settings-label">Codec</label>
              <div className="settings-chip-row">
                {codecOptions.map((codec) => (
                  <button
                    key={codec}
                    className={`settings-chip ${settings.codec === codec ? "active" : ""}`}
                    onClick={() => handleChange("codec", codec)}
                  >
                    {codec}
                  </button>
                ))}
              </div>
            </div>

            {/* Decoder preference */}
            <div className="settings-row settings-row--column">
              <label className="settings-label">Decoder</label>
              <div className="settings-chip-row">
                {accelerationOptions.map((option) => (
                  <button
                    key={`decoder-${option.value}`}
                    className={`settings-chip ${settings.decoderPreference === option.value ? "active" : ""}`}
                    onClick={() => handleChange("decoderPreference", option.value)}
                    title={option.value === "hardware" ? platformHardwareLabel : option.label}
                  >
                    {option.value === "hardware" ? platformHardwareLabel : option.label}
                  </button>
                ))}
              </div>
              <span className="settings-subtle-hint">Applies after app restart.</span>
            </div>

            {/* Encoder preference */}
            <div className="settings-row settings-row--column">
              <label className="settings-label">Encoder</label>
              <div className="settings-chip-row">
                {accelerationOptions.map((option) => (
                  <button
                    key={`encoder-${option.value}`}
                    className={`settings-chip ${settings.encoderPreference === option.value ? "active" : ""}`}
                    onClick={() => handleChange("encoderPreference", option.value)}
                    title={option.value === "hardware" ? platformHardwareLabel : option.label}
                  >
                    {option.value === "hardware" ? platformHardwareLabel : option.label}
                  </button>
                ))}
              </div>
              <span className="settings-subtle-hint">Applies after app restart.</span>
            </div>

            {/* Color Quality */}
            <div className="settings-row settings-row--column">
              <label className="settings-label">Color Depth</label>
              <div className="settings-chip-row">
                {colorQualityOptions.map((opt) => {
                  const needsHevc = colorQualityRequiresHevc(opt.value);
                  return (
                    <button
                      key={opt.value}
                      className={`settings-chip ${settings.colorQuality === opt.value ? "active" : ""}`}
                      onClick={() => handleColorQualityChange(opt.value)}
                      title={`${opt.description}${needsHevc ? " — requires H265/AV1" : ""}`}
                    >
                      <span>{opt.label}</span>
                    </button>
                  );
                })}
              </div>
              {colorQualityRequiresHevc(settings.colorQuality) && settings.codec === "H264" && (
                <span className="settings-input-hint">This mode requires H265 or AV1. Codec will be auto-switched.</span>
              )}
            </div>

            {/* Bitrate slider */}
            <div className="settings-row settings-row--column">
              <div className="settings-row-top">
                <label className="settings-label">Max Bitrate</label>
                <span className="settings-value-badge">{settings.maxBitrateMbps} Mbps</span>
              </div>
              <input
                type="range"
                className="settings-slider"
                min={5}
                max={150}
                step={5}
                value={settings.maxBitrateMbps}
                onChange={(e) => handleChange("maxBitrateMbps", parseInt(e.target.value, 10))}
              />
            </div>

            <div className="settings-row settings-row--column">
              <div className="settings-row-top">
                <label className="settings-label">Session Timer Reappear</label>
                <span className="settings-value-badge">
                  {settings.sessionClockShowEveryMinutes === 0
                    ? "Off"
                    : `Every ${settings.sessionClockShowEveryMinutes} min`}
                </span>
              </div>
              <input
                type="range"
                className="settings-slider"
                min={0}
                max={120}
                step={5}
                value={settings.sessionClockShowEveryMinutes}
                onChange={(e) => handleChange("sessionClockShowEveryMinutes", parseInt(e.target.value, 10))}
              />
              <span className="settings-subtle-hint">
                How often the session timer pops back up while streaming (0 disables repeats).
              </span>
            </div>

            <div className="settings-row settings-row--column">
              <div className="settings-row-top">
                <label className="settings-label">Session Timer Visible Time</label>
                <span className="settings-value-badge">{settings.sessionClockShowDurationSeconds}s</span>
              </div>
              <input
                type="range"
                className="settings-slider"
                min={5}
                max={120}
                step={5}
                value={settings.sessionClockShowDurationSeconds}
                onChange={(e) => handleChange("sessionClockShowDurationSeconds", parseInt(e.target.value, 10))}
              />
              <span className="settings-subtle-hint">
                How long the session timer stays visible each time it appears.
              </span>
            </div>
          </div>
        </section>

        {/* ── Codec Diagnostics ──────────────────────────── */}
        <section className="settings-section">
          <div className="settings-section-header">
            <h2>Codec Diagnostics</h2>
          </div>
          <div className="settings-rows">
            <div className="settings-row codec-test-row">
              <label className="settings-label codec-test-description">
                Test which codecs your system can decode/encode and whether they use GPU or CPU
              </label>
              <button
                className="codec-test-btn"
                onClick={runCodecTest}
                disabled={codecTesting}
                type="button"
              >
                {codecTesting ? (
                  <>
                    <Loader size={16} className="settings-loading-icon" />
                    Testing...
                  </>
                ) : (
                  <>
                    <Zap size={16} />
                    {codecResults ? "Retest" : "Test Codecs"}
                  </>
                )}
              </button>
            </div>

            {codecTestOpen && codecResults && (
              <div className="codec-results">
                {codecResults.map((result) => (
                  <div key={result.codec} className="codec-result-card">
                    <div className="codec-result-header">
                      <span className="codec-result-name">{result.codec}</span>
                      <span className={`codec-result-badge ${result.webrtcSupported ? "supported" : "unsupported"}`}>
                        {result.webrtcSupported ? "WebRTC Ready" : "Not in WebRTC"}
                      </span>
                    </div>

                    <div className="codec-result-rows">
                      {/* Decode row */}
                      <div className="codec-result-row">
                        <span className="codec-result-direction">Decode</span>
                        <span className={`codec-result-status ${result.decodeSupported ? (result.hwAccelerated ? "hw" : "sw") : "none"}`}>
                          {result.decodeSupported
                            ? result.hwAccelerated
                              ? "GPU"
                              : "CPU"
                            : "No"}
                        </span>
                        <span className="codec-result-via">{result.decodeVia}</span>
                      </div>

                      {/* Encode row */}
                      <div className="codec-result-row">
                        <span className="codec-result-direction">Encode</span>
                        <span className={`codec-result-status ${result.encodeSupported ? (result.encodeHwAccelerated ? "hw" : "sw") : "none"}`}>
                          {result.encodeSupported
                            ? result.encodeHwAccelerated
                              ? "GPU"
                              : "CPU"
                            : "No"}
                        </span>
                        <span className="codec-result-via">{result.encodeVia}</span>
                      </div>
                    </div>

                    {/* Profiles */}
                    {result.profiles.length > 0 && (
                      <div className="codec-result-profiles">
                        <span className="codec-result-profiles-label">Profiles:</span>
                        <div className="codec-result-profiles-list">
                          {result.profiles.map((p, i) => (
                            <code key={i} className="codec-result-profile">{p}</code>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        </section>

        {/* ── Audio / Microphone ───────────────────────── */}
        <section className="settings-section">
          <div className="settings-section-header">
            <h2>Audio</h2>
          </div>
          <div className="settings-rows">
            {/* Microphone Mode */}
            <div className="settings-row">
              <label className="settings-label">
                Microphone
                <span className="settings-hint">Enable voice chat during streaming</span>
              </label>
              <div className="settings-dropdown" ref={microphoneModeDropdownRef}>
                <button
                  type="button"
                  className={`settings-dropdown-selected ${microphoneModeDropdownOpen ? "open" : ""}`}
                  onClick={() => {
                    setMicrophoneModeDropdownOpen((open) => !open);
                    setMicrophoneDeviceDropdownOpen(false);
                  }}
                >
                  <span className="settings-dropdown-selected-name">{selectedMicrophoneModeName}</span>
                  <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" className={`settings-dropdown-chevron ${microphoneModeDropdownOpen ? "flipped" : ""}`}>
                    <path d="M4.47 5.97a.75.75 0 0 1 1.06 0L8 8.44l2.47-2.47a.75.75 0 1 1 1.06 1.06l-3 3a.75.75 0 0 1-1.06 0l-3-3a.75.75 0 0 1 0-1.06Z" />
                  </svg>
                </button>
                {microphoneModeDropdownOpen && (
                  <div className="settings-dropdown-menu">
                    {microphoneModeOptions.map((option) => (
                      <button
                        key={option.value}
                        type="button"
                        className={`settings-dropdown-item ${settings.microphoneMode === option.value ? "active" : ""}`}
                        onClick={() => {
                          handleChange("microphoneMode", option.value);
                          setMicrophoneModeDropdownOpen(false);
                        }}
                      >
                        <span>{option.label}</span>
                        {settings.microphoneMode === option.value && <Check size={14} className="settings-dropdown-check" />}
                      </button>
                    ))}
                  </div>
                )}
              </div>
            </div>

            {/* Microphone Device (only shown when mic is enabled) */}
            {settings.microphoneMode !== "disabled" && (
              <div className="settings-row">
                <label className="settings-label">
                  <div className="flex items-center gap-2">
                    <Mic size={14} />
                    Microphone Device
                  </div>
                  <span className="settings-hint">Select input device for voice chat</span>
                </label>
                <div className="settings-mic-device-wrap">
                  <div className="settings-dropdown" ref={microphoneDeviceDropdownRef}>
                    <button
                      type="button"
                      className={`settings-dropdown-selected ${microphoneDeviceDropdownOpen ? "open" : ""}`}
                      onClick={() => {
                        if (microphoneDevices.length === 0) return;
                        setMicrophoneDeviceDropdownOpen((open) => !open);
                        setMicrophoneModeDropdownOpen(false);
                      }}
                      disabled={microphoneDevices.length === 0}
                    >
                      <span className="settings-dropdown-selected-name">{selectedMicrophoneDeviceName}</span>
                      <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" className={`settings-dropdown-chevron ${microphoneDeviceDropdownOpen ? "flipped" : ""}`}>
                        <path d="M4.47 5.97a.75.75 0 0 1 1.06 0L8 8.44l2.47-2.47a.75.75 0 1 1 1.06 1.06l-3 3a.75.75 0 0 1-1.06 0l-3-3a.75.75 0 0 1 0-1.06Z" />
                      </svg>
                    </button>
                    {microphoneDeviceDropdownOpen && (
                      <div className="settings-dropdown-menu settings-dropdown-menu--tall">
                        <button
                          type="button"
                          className={`settings-dropdown-item ${settings.microphoneDeviceId === "" ? "active" : ""}`}
                          onClick={() => {
                            handleChange("microphoneDeviceId", "");
                            setMicrophoneDeviceDropdownOpen(false);
                          }}
                        >
                          <span>Default Device</span>
                          {settings.microphoneDeviceId === "" && <Check size={14} className="settings-dropdown-check" />}
                        </button>
                        {microphoneDevices.map((device, index) => (
                          <button
                            key={device.deviceId}
                            type="button"
                            className={`settings-dropdown-item ${settings.microphoneDeviceId === device.deviceId ? "active" : ""}`}
                            onClick={() => {
                              handleChange("microphoneDeviceId", device.deviceId);
                              setMicrophoneDeviceDropdownOpen(false);
                            }}
                          >
                            <span>{device.label || `Microphone ${index + 1}`}</span>
                            {settings.microphoneDeviceId === device.deviceId && <Check size={14} className="settings-dropdown-check" />}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                  {microphonePermissionError && (
                    <span className="text-red-400 text-xs mt-1">{microphonePermissionError}</span>
                  )}
                  {microphoneDevices.length === 0 && !microphonePermissionError && (
                    <span className="text-yellow-400 text-xs mt-1">No microphone devices found</span>
                  )}
                </div>
              </div>
            )}
          </div>
        </section>

        {/* ── Input ──────────────────────────────────────── */}
        <section className="settings-section">
          <div className="settings-section-header">
            <h2>Input</h2>
          </div>
          <div className="settings-rows">
            <div className="settings-row">
              <label className="settings-label">Clipboard Paste</label>
              <label className="settings-toggle">
                <input
                  type="checkbox"
                  checked={settings.clipboardPaste}
                  onChange={(e) => handleChange("clipboardPaste", e.target.checked)}
                />
                <span className="settings-toggle-track" />
              </label>
            </div>

            <div className="settings-row">
              <label className="settings-label">
                Hide Stream Overlay Buttons
                <span className="settings-hint">Hide microphone, fullscreen, and end-session buttons while streaming.</span>
              </label>
              <label className="settings-toggle">
                <input
                  type="checkbox"
                  checked={settings.hideStreamButtons}
                  onChange={(e) => handleChange("hideStreamButtons", e.target.checked)}
                />
                <span className="settings-toggle-track" />
              </label>
            <div className="settings-row settings-row--column">
              <div className="settings-row">
                <label className="settings-label">
                  <Keyboard size={14} style={{ marginRight: 6, opacity: 0.6 }} />
                  Keyboard Layout
                </label>
              </div>
              <div className="settings-seg">
                {(["auto", "qwerty", "azerty", "qwertz"] as KeyboardLayout[]).map((l) => (
                  <button
                    key={l}
                    className={`settings-seg-btn ${settings.keyboardLayout === l ? "active" : ""}`}
                    onClick={() => handleChange("keyboardLayout", l)}
                  >
                    {l === "auto" ? "Auto" : l.toUpperCase()}
                  </button>
                ))}
              </div>
              <span className="settings-hint">Auto uses the OS layout; override only if keys are wrong in the stream.</span>
            </div>
            <div className="settings-row settings-row--column">
              <label className="settings-label">Microphone</label>

              <div className="settings-seg mic-mode-seg">
                {(["off", "on", "push-to-talk"] as MicMode[]).map((m) => (
                  <button
                    key={m}
                    className={`settings-seg-btn ${settings.micMode === m ? "active" : ""}`}
                    onClick={() => handleChange("micMode", m)}
                  >
                    {m === "off" ? "Off" : m === "on" ? "On" : "Push-to-Talk"}
                  </button>
                ))}
              </div>

              {settings.micMode !== "off" && (
                <>
                  <div className="settings-row" style={{ marginTop: 10 }}>
                    <label className="settings-label">Device</label>
                    <div className="mic-device-select-wrap">
                      <select
                        className="settings-text-input mic-device-select"
                        value={settings.micDeviceId || "default"}
                        onChange={(e) => handleChange("micDeviceId", e.target.value === "default" ? "" : e.target.value)}
                        disabled={micDevicesLoading}
                      >
                        <option value="default">Default Microphone</option>
                        {micDevices
                          .filter((d) => d.deviceId !== "default")
                          .map((d) => (
                            <option key={d.deviceId} value={d.deviceId}>
                              {d.label}
                            </option>
                          ))}
                      </select>
                      <button
                        type="button"
                        className="settings-shortcut-reset-btn mic-refresh-btn"
                        onClick={() => void refreshMicDevices()}
                        disabled={micDevicesLoading}
                        title="Refresh devices"
                      >
                        <RefreshCw size={12} />
                      </button>
                    </div>
                  </div>

                  <div className="settings-row" style={{ marginTop: 6 }}>
                    <label className="settings-label">Input Level</label>
                    <div className="mic-test-meter-wrap">
                      <div className="mic-level-meter">
                        <div
                          className="mic-level-meter-fill"
                          ref={micLevelRef}
                          style={{ transform: `scaleX(${micTestLevel})` }}
                        />
                      </div>
                      <button
                        type="button"
                        className="settings-shortcut-reset-btn mic-test-btn"
                        onClick={() => {
                          if (micTestRunning) {
                            stopMicTest();
                          } else {
                            void startMicTest();
                          }
                        }}
                      >
                        {micTestRunning ? <MicOff size={12} /> : <Mic size={12} />}
                        {micTestRunning ? "Stop" : "Test"}
                      </button>
                    </div>
                    {micTestError && (
                      <span className="mic-test-error">{micTestError}</span>
                    )}
                  </div>

                  <div className="settings-row" style={{ marginTop: 6 }}>
                    <label className="settings-label">Gain</label>
                    <div className="settings-input-group">
                      <input
                        type="range"
                        className="settings-slider"
                        min={0}
                        max={200}
                        step={1}
                        value={Math.round(settings.micGain * 100)}
                        onChange={(e) => {
                          const v = parseInt(e.target.value, 10) / 100;
                          handleChange("micGain", v);
                          if (micTestContextRef.current) {
                            const nodes = micTestContextRef.current.destination;
                            void nodes;
                          }
                        }}
                      />
                      <span className="settings-value-badge">{Math.round(settings.micGain * 100)}%</span>
                    </div>
                  </div>

                  <div className="settings-row" style={{ marginTop: 6 }}>
                    <label className="settings-label">Noise Suppression</label>
                    <label className="settings-toggle">
                      <input
                        type="checkbox"
                        checked={settings.micNoiseSuppression}
                        onChange={(e) => handleChange("micNoiseSuppression", e.target.checked)}
                      />
                      <span className="settings-toggle-track" />
                    </label>
                  </div>

                  <div className="settings-row">
                    <label className="settings-label">Echo Cancellation</label>
                    <label className="settings-toggle">
                      <input
                        type="checkbox"
                        checked={settings.micEchoCancellation}
                        onChange={(e) => handleChange("micEchoCancellation", e.target.checked)}
                      />
                      <span className="settings-toggle-track" />
                    </label>
                  </div>

                  <div className="settings-row">
                    <label className="settings-label">Auto Gain Control</label>
                    <label className="settings-toggle">
                      <input
                        type="checkbox"
                        checked={settings.micAutoGainControl}
                        onChange={(e) => handleChange("micAutoGainControl", e.target.checked)}
                      />
                      <span className="settings-toggle-track" />
                    </label>
                  </div>

                </>
              )}            </div>

            <div className="settings-row settings-row--column">
              <div className="settings-row-top">
                <label className="settings-label">Shortcuts</label>
                <div className="settings-shortcut-actions">
                  <span className="settings-value-badge">Editable</span>
                  <button
                    type="button"
                    className="settings-shortcut-reset-btn"
                    onClick={handleResetShortcuts}
                    disabled={areShortcutsDefault}
                  >
                    Reset to defaults
                  </button>
                </div>
              </div>

              <div className="settings-shortcut-grid">
                <label className="settings-shortcut-row">
                  <span className="settings-shortcut-label">Toggle Stats</span>
                  <input
                    type="text"
                    className={`settings-text-input settings-shortcut-input ${toggleStatsError ? "error" : ""}`}
                    value={toggleStatsInput}
                    onChange={(e) => setToggleStatsInput(e.target.value)}
                    onBlur={() => handleShortcutBlur("shortcutToggleStats", toggleStatsInput, setToggleStatsInput, setToggleStatsError)}
                    onKeyDown={handleShortcutKeyDown}
                    placeholder="F3"
                    spellCheck={false}
                  />
                </label>

                <label className="settings-shortcut-row">
                  <span className="settings-shortcut-label">Mouse Lock</span>
                  <input
                    type="text"
                    className={`settings-text-input settings-shortcut-input ${togglePointerLockError ? "error" : ""}`}
                    value={togglePointerLockInput}
                    onChange={(e) => setTogglePointerLockInput(e.target.value)}
                    onBlur={() => handleShortcutBlur("shortcutTogglePointerLock", togglePointerLockInput, setTogglePointerLockInput, setTogglePointerLockError)}
                    onKeyDown={handleShortcutKeyDown}
                    placeholder="F8"
                    spellCheck={false}
                  />
                </label>

                <label className="settings-shortcut-row">
                  <span className="settings-shortcut-label">Stop Stream</span>
                  <input
                    type="text"
                    className={`settings-text-input settings-shortcut-input ${stopStreamError ? "error" : ""}`}
                    value={stopStreamInput}
                    onChange={(e) => setStopStreamInput(e.target.value)}
                    onBlur={() => handleShortcutBlur("shortcutStopStream", stopStreamInput, setStopStreamInput, setStopStreamError)}
                    onKeyDown={handleShortcutKeyDown}
                    placeholder="Ctrl+Shift+Q"
                    spellCheck={false}
                  />
                </label>

                <label className="settings-shortcut-row">
                  <span className="settings-shortcut-label">Toggle Anti-AFK</span>
                  <input
                    type="text"
                    className={`settings-text-input settings-shortcut-input ${toggleAntiAfkError ? "error" : ""}`}
                    value={toggleAntiAfkInput}
                    onChange={(e) => setToggleAntiAfkInput(e.target.value)}
                    onBlur={() => handleShortcutBlur("shortcutToggleAntiAfk", toggleAntiAfkInput, setToggleAntiAfkInput, setToggleAntiAfkError)}
                    onKeyDown={handleShortcutKeyDown}
                    placeholder="Ctrl+Shift+K"
                    spellCheck={false}
                  />
                </label>

                <label className="settings-shortcut-row">
                  <span className="settings-shortcut-label">Toggle Microphone</span>
                  <input
                    type="text"
                    className={`settings-text-input settings-shortcut-input ${toggleMicrophoneError ? "error" : ""}`}
                    value={toggleMicrophoneInput}
                    onChange={(e) => setToggleMicrophoneInput(e.target.value)}
                    onBlur={() => handleShortcutBlur("shortcutToggleMicrophone", toggleMicrophoneInput, setToggleMicrophoneInput, setToggleMicrophoneError)}
                    onKeyDown={handleShortcutKeyDown}
                    placeholder="Ctrl+Shift+M"
                    spellCheck={false}
                  />
                </label>
              </div>

              {(toggleStatsError || togglePointerLockError || stopStreamError || toggleAntiAfkError || toggleMicrophoneError) && (
                <span className="settings-input-hint">
                  Invalid shortcut. Use {shortcutExamples}
                </span>
              )}

              {!toggleStatsError && !togglePointerLockError && !stopStreamError && !toggleAntiAfkError && !toggleMicrophoneError && (
                <span className="settings-shortcut-hint">
                  {shortcutExamples}. Stop: {formatShortcutForDisplay(settings.shortcutStopStream, isMac)}. Mic: {formatShortcutForDisplay(settings.shortcutToggleMicrophone, isMac)}.
                </span>
              )}
            </div>
          </div>
        </section>

        {/* ── Region ────────────────────────────────────── */}
        <section className="settings-section">
          <div className="settings-section-header">
            <h2>Region</h2>
          </div>
          <div className="settings-rows">
            {/* Region selector with search */}
            <div className="region-selector">
              <button
                className={`region-selected ${regionDropdownOpen ? "open" : ""}`}
                onClick={() => setRegionDropdownOpen(!regionDropdownOpen)}
                type="button"
              >
                <span className="region-selected-name">{selectedRegionName}</span>
                <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" className={`region-chevron ${regionDropdownOpen ? "flipped" : ""}`}>
                  <path d="M4.47 5.97a.75.75 0 0 1 1.06 0L8 8.44l2.47-2.47a.75.75 0 1 1 1.06 1.06l-3 3a.75.75 0 0 1-1.06 0l-3-3a.75.75 0 0 1 0-1.06Z" />
                </svg>
              </button>

              {regionDropdownOpen && (
                <div className="region-dropdown">
                  <div className="region-dropdown-search">
                    <Search size={14} className="region-dropdown-search-icon" />
                    <input
                      type="text"
                      className="region-dropdown-search-input"
                      placeholder="Search regions..."
                      value={regionSearch}
                      onChange={(e) => setRegionSearch(e.target.value)}
                      autoFocus
                    />
                    {regionSearch && (
                      <button className="region-dropdown-clear" onClick={() => setRegionSearch("")} type="button">
                        <X size={12} />
                      </button>
                    )}
                  </div>

                  <div className="region-dropdown-list">
                    <button
                      className={`region-dropdown-item ${!settings.region ? "active" : ""}`}
                      onClick={() => {
                        handleChange("region", "");
                        setRegionDropdownOpen(false);
                        setRegionSearch("");
                      }}
                      type="button"
                    >
                      <Globe size={14} />
                      <span>Auto (Best)</span>
                      {!settings.region && <Check size={14} className="region-check" />}
                    </button>

                    {filteredRegions.map((region) => (
                      <button
                        key={region.url}
                        className={`region-dropdown-item ${settings.region === region.url ? "active" : ""}`}
                        onClick={() => {
                          handleChange("region", region.url);
                          setRegionDropdownOpen(false);
                          setRegionSearch("");
                        }}
                        type="button"
                      >
                        <Globe size={14} />
                        <span>{region.name}</span>
                        {settings.region === region.url && <Check size={14} className="region-check" />}
                      </button>
                    ))}

                    {filteredRegions.length === 0 && regions.length > 0 && (
                      <div className="region-dropdown-empty">No regions match &ldquo;{regionSearch}&rdquo;</div>
                    )}
                  </div>
                </div>
              )}
            </div>
          </div>
        </section>

        {/* ── Miscellaneous ──────────────────────────────── */}
        <section className="settings-section">
          <div className="settings-section-header">
            <h2>Miscellaneous</h2>
          </div>
          <div className="settings-rows">
            {/* Export Logs */}
            <div className="settings-row">
              <label className="settings-label">
                Export Logs
                <span className="settings-hint">Download debug logs with sensitive data redacted for privacy</span>
              </label>
              <button
                type="button"
                className="settings-export-logs-btn"
                onClick={async () => {
                  try {
                    const logs = await window.openNow.exportLogs("text");
                    const blob = new Blob([logs], { type: "text/plain" });
                    const url = URL.createObjectURL(blob);
                    const a = document.createElement("a");
                    a.href = url;
                    a.download = `opennow-logs-${new Date().toISOString().replace(/[:.]/g, "-")}.txt`;
                    document.body.appendChild(a);
                    a.click();
                    document.body.removeChild(a);
                    URL.revokeObjectURL(url);
                  } catch (err) {
                    console.error("[Settings] Failed to export logs:", err);
                    alert("Failed to export logs. Please try again.");
                  }
                }}
              >
                <FileDown size={16} />
                Export Logs
              </button>
            </div>
          </div>
        </section>
      </div>

      {/* Footer */}
      <div className="settings-footer">
        <button
          className="settings-save-btn"
          onClick={() => {
            setSavedIndicator(true);
            setTimeout(() => setSavedIndicator(false), 1500);
          }}
        >
          <Save size={16} />
          Save Settings
        </button>
      </div>
    </div>
  );
}
