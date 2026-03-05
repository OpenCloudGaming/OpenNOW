import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import type { JSX } from "react";
import { Maximize, Minimize, Gamepad2, Loader2, LogOut, Clock3, AlertTriangle, Mic, MicOff, Camera, ChevronLeft, ChevronRight, Save, Trash2, X } from "lucide-react";
import SideBar from "./SideBar";
import type { StreamDiagnostics } from "../gfn/webrtcClient";
import { getStoreDisplayName, getStoreIconComponent } from "./GameCard";
import type { MicrophoneMode, ScreenshotEntry } from "@shared/gfn";
import { isShortcutMatch, normalizeShortcut } from "../shortcuts";

interface StreamViewProps {
  videoRef: React.Ref<HTMLVideoElement>;
  audioRef: React.Ref<HTMLAudioElement>;
  stats: StreamDiagnostics;
  showStats: boolean;
  shortcuts: {
    toggleStats: string;
    togglePointerLock: string;
    stopStream: string;
    toggleMicrophone?: string;
    screenshot: string;
  };
  hideStreamButtons?: boolean;
  serverRegion?: string;
  connectedControllers: number;
  antiAfkEnabled: boolean;
  escHoldReleaseIndicator: {
    visible: boolean;
    progress: number;
  };
  exitPrompt: {
    open: boolean;
    gameTitle: string;
  };
  sessionElapsedSeconds: number;
  sessionClockShowEveryMinutes: number;
  sessionClockShowDurationSeconds: number;
  streamWarning: {
    code: 1 | 2 | 3;
    message: string;
    tone: "warn" | "critical";
    secondsLeft?: number;
  } | null;
  isConnecting: boolean;
  gameTitle: string;
  platformStore?: string;
  onToggleFullscreen: () => void;
  onConfirmExit: () => void;
  onCancelExit: () => void;
  onEndSession: () => void;
  onToggleMicrophone?: () => void;
  mouseSensitivity: number;
  onMouseSensitivityChange: (value: number) => void;
  mouseAcceleration: number;
  onMouseAccelerationChange: (value: number) => void;
  onRequestPointerLock?: () => void;
  onReleasePointerLock?: () => void;
  microphoneMode: MicrophoneMode;
  onMicrophoneModeChange: (value: MicrophoneMode) => void;
  onScreenshotShortcutChange: (value: string) => void;
  remainingPlaytimeText: string;
  micTrack?: MediaStreamTrack | null;
}

function getRttColor(rttMs: number): string {
  if (rttMs <= 0) return "var(--ink-muted)";
  if (rttMs < 30) return "var(--success)";
  if (rttMs < 60) return "var(--warning)";
  return "var(--error)";
}

function getPacketLossColor(lossPercent: number): string {
  if (lossPercent <= 0.15) return "var(--success)";
  if (lossPercent < 1) return "var(--warning)";
  return "var(--error)";
}

function getTimingColor(valueMs: number, goodMax: number, warningMax: number): string {
  if (valueMs <= 0) return "var(--ink-muted)";
  if (valueMs <= goodMax) return "var(--success)";
  if (valueMs <= warningMax) return "var(--warning)";
  return "var(--error)";
}

function getInputQueueColor(bufferedBytes: number, dropCount: number): string {
  if (dropCount > 0 || bufferedBytes >= 65536) return "var(--error)";
  if (bufferedBytes >= 32768) return "var(--warning)";
  return "var(--success)";
}

function formatElapsed(totalSeconds: number): string {
  const safe = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(safe / 3600);
  const minutes = Math.floor((safe % 3600) / 60);
  const seconds = safe % 60;
  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, "0")}:${seconds.toString().padStart(2, "0")}`;
  }
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

function formatWarningSeconds(value: number | undefined): string | null {
  if (value === undefined || !Number.isFinite(value) || value < 0) {
    return null;
  }
  const total = Math.floor(value);
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  if (minutes > 0) {
    return `${minutes}m ${seconds.toString().padStart(2, "0")}s`;
  }
  return `${seconds}s`;
}

/**
 * Drives a canvas-based segmented level meter from a live MediaStreamTrack.
 * Uses the Web Audio API AnalyserNode as a read-only tap — audio is never
 * routed to the speaker. Runs a requestAnimationFrame loop while active;
 * tears down fully (rAF cancelled, AudioContext closed) on deactivation.
 */
function useMicMeter(
  canvasRef: React.RefObject<HTMLCanvasElement | null>,
  track: MediaStreamTrack | null,
  active: boolean,
): void {
  const pendingCloseRef = useRef<Promise<void> | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!active || !track || !canvas) return;

    const ctx2d = canvas.getContext("2d");
    if (!ctx2d) return;

    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(canvas.clientWidth * dpr);
    canvas.height = Math.round(canvas.clientHeight * dpr);
    const W = canvas.width;
    const H = canvas.height;
    if (W <= 0 || H <= 0) {
      return;
    }

    let audioCtx: AudioContext | null = null;
    let source: MediaStreamAudioSourceNode | null = null;
    let analyser: AnalyserNode | null = null;
    let raf = 0;
    let dead = false;

    const start = async () => {
      if (pendingCloseRef.current) {
        try {
          await pendingCloseRef.current;
        } catch {
          // Ignore close errors from previous contexts.
        }
      }
      if (dead) {
        return;
      }

      try {
        audioCtx = new AudioContext();
        await audioCtx.resume().catch(() => undefined);
        if (dead) {
          return;
        }

        analyser = audioCtx.createAnalyser();
        analyser.fftSize = 512;
        analyser.smoothingTimeConstant = 0.65;
        source = audioCtx.createMediaStreamSource(new MediaStream([track]));
        source.connect(analyser);
        // NOT connected to destination — monitoring only, no loopback

        const buf = new Uint8Array(analyser.frequencyBinCount);
        const SEG = 20;
        const GAP = Math.round(2 * dpr);
        const bw = (W - GAP * (SEG - 1)) / SEG;
        const radius = Math.min(3 * dpr, bw / 2);

        const frame = () => {
          if (dead || !analyser) return;
          raf = requestAnimationFrame(frame);
          analyser.getByteTimeDomainData(buf);

          let sum = 0;
          for (let i = 0; i < buf.length; i++) {
            const v = ((buf[i] ?? 128) - 128) / 128;
            sum += v * v;
          }
          const rms = Math.sqrt(sum / buf.length);
          const level = Math.min(1, rms * 5.5);
          const filled = Math.round(level * SEG);

          ctx2d.clearRect(0, 0, W, H);
          for (let i = 0; i < SEG; i++) {
            const x = i * (bw + GAP);
            if (i < filled) {
              ctx2d.fillStyle =
                i < SEG * 0.7 ? "#58d98a" : i < SEG * 0.9 ? "#fbbf24" : "#f87171";
            } else {
              ctx2d.fillStyle = "rgba(255,255,255,0.07)";
            }
            ctx2d.beginPath();
            ctx2d.roundRect(x, 0, Math.max(1, bw), H, radius);
            ctx2d.fill();
          }
        };

        frame();
      } catch (e) {
        console.warn("[MicMeter]", e);
      }
    };

    void start();

    return () => {
      dead = true;
      cancelAnimationFrame(raf);
      source?.disconnect();
      analyser?.disconnect();
      if (audioCtx && audioCtx.state !== "closed") {
        pendingCloseRef.current = audioCtx
          .close()
          .catch(() => undefined)
          .then(() => undefined);
      }
    };
  }, [track, active, canvasRef]);
}

export function StreamView({
  videoRef,
  audioRef,
  stats,
  showStats,
  shortcuts,
  serverRegion,
  connectedControllers,
  antiAfkEnabled,
  escHoldReleaseIndicator,
  exitPrompt,
  sessionElapsedSeconds,
  sessionClockShowEveryMinutes,
  sessionClockShowDurationSeconds,
  streamWarning,
  isConnecting,
  gameTitle,
  platformStore,
  onToggleFullscreen,
  onConfirmExit,
  onCancelExit,
  onEndSession,
  onToggleMicrophone,
  mouseSensitivity,
  onMouseSensitivityChange,
  mouseAcceleration,
  onMouseAccelerationChange,
  onRequestPointerLock,
  onReleasePointerLock,
  microphoneMode,
  onMicrophoneModeChange,
  onScreenshotShortcutChange,
  remainingPlaytimeText,
  micTrack,
  hideStreamButtons = false,
}: StreamViewProps): JSX.Element {
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [showHints, setShowHints] = useState(true);
  const [showSessionClock, setShowSessionClock] = useState(false);
  const [showSideBar, setShowSideBar] = useState(false);
  const [isPointerLocked, setIsPointerLocked] = useState(false);
  const [screenshots, setScreenshots] = useState<ScreenshotEntry[]>([]);
  const [isSavingScreenshot, setIsSavingScreenshot] = useState(false);
  const [galleryError, setGalleryError] = useState<string | null>(null);
  const [selectedScreenshotId, setSelectedScreenshotId] = useState<string | null>(null);
  const [screenshotShortcutInput, setScreenshotShortcutInput] = useState(shortcuts.screenshot);
  const [screenshotShortcutError, setScreenshotShortcutError] = useState<string | null>(null);
  const [activeSidebarTab, setActiveSidebarTab] = useState<"preferences" | "shortcuts">("preferences");
  const screenshotApiAvailable =
    typeof window.openNow?.saveScreenshot === "function" &&
    typeof window.openNow?.listScreenshots === "function" &&
    typeof window.openNow?.deleteScreenshot === "function" &&
    typeof window.openNow?.saveScreenshotAs === "function";

  // Microphone state
  const micState = stats.micState ?? "uninitialized";
  const micEnabled = stats.micEnabled ?? false;
  const hasMicrophone = micState === "started" || micState === "stopped";
  const showMicIndicator = hasMicrophone && !isConnecting && !hideStreamButtons;
  const microphoneModes = useMemo(
    () => [
      { value: "disabled" as MicrophoneMode, label: "Disabled", description: "No microphone input" },
      { value: "push-to-talk" as MicrophoneMode, label: "Push-to-Talk", description: "Hold a key to talk" },
      { value: "voice-activity" as MicrophoneMode, label: "Voice Activity", description: "Always listen" },
    ],
    []
  );

  const handleFullscreenToggle = useCallback(() => {
    onToggleFullscreen();
  }, [onToggleFullscreen]);

  const handlePointerLockToggle = useCallback(() => {
    if (isPointerLocked) {
      document.exitPointerLock();
      return;
    }
    if (onRequestPointerLock) {
      onRequestPointerLock();
    }
  }, [isPointerLocked, onRequestPointerLock]);

  useEffect(() => {
    const timer = setTimeout(() => setShowHints(false), 5000);
    return () => clearTimeout(timer);
  }, []);

  useEffect(() => {
    const handleFullscreenChange = () => {
      setIsFullscreen(!!document.fullscreenElement);
    };
    document.addEventListener("fullscreenchange", handleFullscreenChange);
    return () => document.removeEventListener("fullscreenchange", handleFullscreenChange);
  }, []);

  useEffect(() => {
    if (isConnecting) {
      setShowSessionClock(false);
      return;
    }

    const intervalMinutes = Math.max(0, Math.floor(sessionClockShowEveryMinutes || 0));
    const durationSeconds = Math.max(1, Math.floor(sessionClockShowDurationSeconds || 1));
    const intervalMs = intervalMinutes * 60 * 1000;
    const durationMs = durationSeconds * 1000;

    let hideTimer: number | undefined;
    let periodicTimer: number | undefined;

    const showFor = (durationMs: number): void => {
      setShowSessionClock(true);
      if (hideTimer !== undefined) {
        window.clearTimeout(hideTimer);
      }
      hideTimer = window.setTimeout(() => {
        setShowSessionClock(false);
      }, durationMs);
    };

    // Show session clock at stream start.
    showFor(durationMs);

    if (intervalMs > 0) {
      periodicTimer = window.setInterval(() => {
        showFor(durationMs);
      }, intervalMs);
    }

    return () => {
      if (hideTimer !== undefined) {
        window.clearTimeout(hideTimer);
      }
      if (periodicTimer !== undefined) {
        window.clearInterval(periodicTimer);
      }
    };
  }, [isConnecting, sessionClockShowDurationSeconds, sessionClockShowEveryMinutes]);

  const bitrateMbps = (stats.bitrateKbps / 1000).toFixed(1);
  const hasResolution = stats.resolution && stats.resolution !== "";
  const hasCodec = stats.codec && stats.codec !== "";
  const regionLabel = stats.serverRegion || serverRegion || "";
  const decodeColor = getTimingColor(stats.decodeTimeMs, 8, 16);
  const renderColor = getTimingColor(stats.renderTimeMs, 12, 22);
  const jitterBufferColor = getTimingColor(stats.jitterBufferDelayMs, 10, 24);
  const lossColor = getPacketLossColor(stats.packetLossPercent);
  const dText = stats.decodeTimeMs > 0 ? `${stats.decodeTimeMs.toFixed(1)}ms` : "--";
  const rText = stats.renderTimeMs > 0 ? `${stats.renderTimeMs.toFixed(1)}ms` : "--";
  const jbText = stats.jitterBufferDelayMs > 0 ? `${stats.jitterBufferDelayMs.toFixed(1)}ms` : "--";
  const inputLive = stats.inputReady && stats.connectionState === "connected";
  const escHoldProgress = Math.max(0, Math.min(1, escHoldReleaseIndicator.progress));
  const escHoldSecondsLeft = Math.max(0, 5 - Math.floor(escHoldProgress * 5));
  const inputQueueColor = getInputQueueColor(stats.inputQueueBufferedBytes, stats.inputQueueDropCount);
  const inputQueueText = `${(stats.inputQueueBufferedBytes / 1024).toFixed(1)}KB`;
  const warningSeconds = formatWarningSeconds(streamWarning?.secondsLeft);
  const sessionTimeText = formatElapsed(sessionElapsedSeconds);
  const platformName = platformStore ? getStoreDisplayName(platformStore) : "";
  const PlatformIcon = platformStore ? getStoreIconComponent(platformStore) : null;
  const isMacClient = navigator.platform?.toLowerCase().includes("mac") || navigator.userAgent.includes("Macintosh");

  // Local ref for video element to manage focus
  const localVideoRef = useRef<HTMLVideoElement | null>(null);

  // Mic level meter canvas
  const micMeterRef = useRef<HTMLCanvasElement | null>(null);
  const galleryStripRef = useRef<HTMLDivElement | null>(null);
  useMicMeter(micMeterRef, micTrack ?? null, showSideBar && microphoneMode !== "disabled");

  const selectedScreenshot = useMemo(() => {
    if (!selectedScreenshotId) return null;
    return screenshots.find((item) => item.id === selectedScreenshotId) ?? null;
  }, [screenshots, selectedScreenshotId]);

  useEffect(() => {
    setScreenshotShortcutInput(shortcuts.screenshot);
    setScreenshotShortcutError(null);
  }, [shortcuts.screenshot]);

  const getScreenshotShortcutError = useCallback((rawValue: string): string | null => {
    const trimmed = rawValue.trim();
    if (!trimmed) {
      return "Shortcut cannot be empty.";
    }

    const normalized = normalizeShortcut(trimmed);
    if (!normalized.valid) {
      return "Invalid shortcut format.";
    }

    const reserved = [
      shortcuts.toggleStats,
      shortcuts.togglePointerLock,
      shortcuts.stopStream,
      shortcuts.toggleMicrophone,
      isMacClient ? "Cmd+G" : "Ctrl+Shift+G",
    ]
      .filter((value): value is string => typeof value === "string" && value.trim().length > 0)
      .map((value) => normalizeShortcut(value))
      .filter((parsed) => parsed.valid)
      .map((parsed) => parsed.canonical);

    if (reserved.includes(normalized.canonical)) {
      return "Shortcut conflicts with an existing binding.";
    }

    return null;
  }, [isMacClient, shortcuts.stopStream, shortcuts.toggleMicrophone, shortcuts.togglePointerLock, shortcuts.toggleStats]);

  const refreshScreenshots = useCallback(async () => {
    setGalleryError(null);
    if (!screenshotApiAvailable) {
      setGalleryError("Screenshot API unavailable. Restart OpenNOW to enable gallery.");
      return;
    }
    try {
      const items = await window.openNow.listScreenshots();
      setScreenshots(items);
    } catch (error) {
      console.error("[StreamView] Failed to load screenshots:", error);
      setGalleryError("Unable to load screenshot gallery.");
    }
  }, [screenshotApiAvailable]);

  const captureScreenshot = useCallback(async () => {
    setGalleryError(null);
    if (!screenshotApiAvailable) {
      setGalleryError("Screenshot API unavailable. Restart OpenNOW to enable capture.");
      return;
    }
    if (isSavingScreenshot) {
      return;
    }

    const video = localVideoRef.current;
    if (!video || video.videoWidth <= 0 || video.videoHeight <= 0) {
      setGalleryError("Stream is not ready for screenshots yet.");
      return;
    }

    setIsSavingScreenshot(true);
    try {
      const canvas = document.createElement("canvas");
      canvas.width = video.videoWidth;
      canvas.height = video.videoHeight;
      const context = canvas.getContext("2d");
      if (!context) {
        throw new Error("Could not acquire 2D context");
      }

      context.drawImage(video, 0, 0, canvas.width, canvas.height);
      const dataUrl = canvas.toDataURL("image/png");
      const saved = await window.openNow.saveScreenshot({ dataUrl, gameTitle });
      setScreenshots((prev) => [saved, ...prev.filter((item) => item.id !== saved.id)].slice(0, 60));
    } catch (error) {
      console.error("[StreamView] Failed to capture screenshot:", error);
      setGalleryError("Screenshot failed. Try again.");
    } finally {
      setIsSavingScreenshot(false);
    }
  }, [gameTitle, isSavingScreenshot, screenshotApiAvailable]);

  const scrollGallery = useCallback((direction: "left" | "right") => {
    const strip = galleryStripRef.current;
    if (!strip) return;
    const delta = Math.max(180, Math.round(strip.clientWidth * 0.7));
    strip.scrollBy({ left: direction === "left" ? -delta : delta, behavior: "smooth" });
  }, []);

  const handleDeleteScreenshot = useCallback(async () => {
    setGalleryError(null);
    if (!screenshotApiAvailable) {
      setGalleryError("Screenshot API unavailable. Restart OpenNOW to enable gallery.");
      return;
    }
    if (!selectedScreenshot) return;

    try {
      await window.openNow.deleteScreenshot({ id: selectedScreenshot.id });
      setScreenshots((prev) => prev.filter((item) => item.id !== selectedScreenshot.id));
      setSelectedScreenshotId(null);
    } catch (error) {
      console.error("[StreamView] Failed to delete screenshot:", error);
      setGalleryError("Unable to delete screenshot.");
    }
  }, [screenshotApiAvailable, selectedScreenshot]);

  const handleSaveScreenshotAs = useCallback(async () => {
    setGalleryError(null);
    if (!screenshotApiAvailable) {
      setGalleryError("Screenshot API unavailable. Restart OpenNOW to enable gallery.");
      return;
    }
    if (!selectedScreenshot) return;

    try {
      await window.openNow.saveScreenshotAs({ id: selectedScreenshot.id });
    } catch (error) {
      console.error("[StreamView] Failed to save screenshot as:", error);
      setGalleryError("Unable to save screenshot.");
    }
  }, [screenshotApiAvailable, selectedScreenshot]);

  const setVideoRef = useCallback((element: HTMLVideoElement | null) => {
    localVideoRef.current = element;
    if (typeof videoRef === "function") {
      videoRef(element);
    } else if (videoRef && "current" in videoRef) {
      (videoRef as React.MutableRefObject<HTMLVideoElement | null>).current = element;
    }
  }, [videoRef]);

  useEffect(() => {
    const handlePointerLockChange = () => {
      setIsPointerLocked(document.pointerLockElement === localVideoRef.current);
    };
    document.addEventListener("pointerlockchange", handlePointerLockChange);
    return () => document.removeEventListener("pointerlockchange", handlePointerLockChange);
  }, []);

  useEffect(() => {
    if (showSideBar) {
      document.exitPointerLock();
      void refreshScreenshots();
      return;
    }
    // Sidebar just closed — restore focus to the video so clicks register
    // immediately. Without this, focus stays on the last sidebar element and
    // mousedown's preventDefault() blocks the browser from re-focusing on click.
    const timer = window.setTimeout(() => {
      if (localVideoRef.current && document.activeElement !== localVideoRef.current) {
        localVideoRef.current.focus();
      }
    }, 50);
    return () => clearTimeout(timer);
  }, [refreshScreenshots, showSideBar]);

  useEffect(() => {
    if (!selectedScreenshotId) return;
    if (!screenshots.some((item) => item.id === selectedScreenshotId)) {
      setSelectedScreenshotId(null);
    }
  }, [screenshots, selectedScreenshotId]);

  useEffect(() => {
    if (!isConnecting && localVideoRef.current && hasResolution) {
      const timer = window.setTimeout(() => {
        if (localVideoRef.current && document.activeElement !== localVideoRef.current) {
          localVideoRef.current.focus();
          console.log("[StreamView] Focused video element");
        }
      }, 100);
      return () => clearTimeout(timer);
    }
  }, [isConnecting, hasResolution]);

  const handleToggleSideBar = useCallback(() => {
    setShowSideBar((s) => {
      if (!s && document.pointerLockElement) {
        if (onReleasePointerLock) {
          onReleasePointerLock();
        } else {
          document.exitPointerLock();
        }
      }
      return !s;
    });
  }, [onReleasePointerLock]);

  useEffect(() => {
    const screenshotShortcut = normalizeShortcut(shortcuts.screenshot);
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const isTyping = !!target && (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable
      );
      if (isTyping) {
        return;
      }

      if (isShortcutMatch(event, screenshotShortcut)) {
        event.preventDefault();
        event.stopPropagation();
        void captureScreenshot();
        return;
      }

      const key = event.key.toLowerCase();
      if (isMacClient) {
        if (event.metaKey && !event.ctrlKey && !event.shiftKey && key === "g") {
          event.preventDefault();
          handleToggleSideBar();
        }
      } else if (event.ctrlKey && event.shiftKey && !event.metaKey && key === "g") {
        event.preventDefault();
        handleToggleSideBar();
      }
    };

    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [captureScreenshot, handleToggleSideBar, isMacClient, shortcuts.screenshot]);

  return (
    <div className="sv">
      <video
        ref={setVideoRef}
        autoPlay
        playsInline
        muted
        tabIndex={0}
        className="sv-video"
        onClick={() => {
          if (localVideoRef.current && document.activeElement !== localVideoRef.current) {
            localVideoRef.current.focus();
          }
        }}
      />
      <audio ref={audioRef} autoPlay playsInline />

      {showSideBar && (
        <>
          <div
            className="sv-sidebar-backdrop"
            onMouseDown={(event) => event.stopPropagation()}
            onClick={() => setShowSideBar(false)}
          />
          <SideBar title="Settings" className="sv-sidebar" onClose={() => setShowSideBar(false)}>
            <div className="sidebar-stat-line" title="Total remaining playtime from subscription">
              <span className="sidebar-stat-label">Remaining Playtime</span>
              <span className="settings-value-badge">{remainingPlaytimeText}</span>
            </div>
            <div className="sidebar-tabs" role="tablist" aria-label="Sidebar sections">
              <button
                type="button"
                role="tab"
                aria-selected={activeSidebarTab === "preferences"}
                className={`sidebar-tab${activeSidebarTab === "preferences" ? " sidebar-tab--active" : ""}`}
                onClick={() => setActiveSidebarTab("preferences")}
              >
                Preferences
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={activeSidebarTab === "shortcuts"}
                className={`sidebar-tab${activeSidebarTab === "shortcuts" ? " sidebar-tab--active" : ""}`}
                onClick={() => setActiveSidebarTab("shortcuts")}
              >
                Shortcuts
              </button>
            </div>

            {activeSidebarTab === "preferences" && (
              <>
                <div className="sidebar-separator" aria-hidden="true" />
                <section className="sidebar-section">
                  <div className="sidebar-section-header">
                    <span>Mouse Preferences</span>
                    <span className="sidebar-section-sub">Fine-tune cursor movement</span>
                  </div>
                  <div className="sidebar-row sidebar-row--column">
                    <div className="sidebar-row-top">
                      <span className="sidebar-label">Mouse Sensitivity</span>
                      <span className="settings-value-badge">{mouseSensitivity.toFixed(2)}x</span>
                    </div>
                    <input
                      type="range"
                      className="settings-slider"
                      min={0.1}
                      max={4}
                      step={0.01}
                      value={mouseSensitivity}
                      onChange={(event) => {
                        const next = Number(event.target.value);
                        if (Number.isFinite(next)) {
                          onMouseSensitivityChange(Math.max(0.1, Math.min(4, next)));
                        }
                      }}
                    />
                    <span className="sidebar-hint">Multiplier applied to mouse movement (1.00 = default).</span>
                  </div>
                  <div className="sidebar-row sidebar-row--column">
                    <div className="sidebar-row-top">
                      <span className="sidebar-label">Mouse Accelerator</span>
                      <span className="settings-value-badge">{Math.round(mouseAcceleration)}%</span>
                    </div>
                    <input
                      type="range"
                      className="settings-slider"
                      min={1}
                      max={150}
                      step={1}
                      value={Math.round(mouseAcceleration)}
                      onChange={(event) => {
                        const next = Number(event.target.value);
                        if (Number.isFinite(next)) {
                          onMouseAccelerationChange(Math.max(1, Math.min(150, Math.round(next))));
                        }
                      }}
                    />
                    <span className="sidebar-hint">Dynamic turn boost strength (1% = off-like, 150% = strongest).</span>
                  </div>
                </section>
                <div className="sidebar-separator" aria-hidden="true" />
                <section className="sidebar-section">
                  <div className="sidebar-section-header">
                    <span>Audio</span>
                    <span className="sidebar-section-sub">Microphone handling</span>
                  </div>
                  <div className="sidebar-row sidebar-row--column">
                    <div className="sidebar-row-top">
                      <span className="sidebar-label">Microphone Mode</span>
                      <span className="settings-value-badge">
                        {microphoneModes.find((option) => option.value === microphoneMode)?.label ?? microphoneMode}
                      </span>
                    </div>
                    <div className="sidebar-chip-row">
                      {microphoneModes.map((option) => (
                        <button
                          key={option.value}
                          type="button"
                          className={`sidebar-chip${microphoneMode === option.value ? " sidebar-chip--active" : ""}`}
                          onClick={() => onMicrophoneModeChange(option.value)}
                        >
                          <span>{option.label}</span>
                        </button>
                      ))}
                    </div>
                    <span className="sidebar-hint">
                      {microphoneModes.find((option) => option.value === microphoneMode)?.description ?? ""}
                    </span>
                  </div>
                  {microphoneMode !== "disabled" && (
                    <div className="sidebar-row sidebar-row--column">
                      <div className="sidebar-row-top">
                        <span className="sidebar-label">Input Level</span>
                        {micTrack && !micEnabled && <span className="settings-value-badge">Muted</span>}
                      </div>
                      <canvas
                        ref={micMeterRef}
                        className="mic-meter-canvas"
                        aria-label="Microphone input level"
                      />
                      {!micTrack && <span className="sidebar-hint">Mic not active — check mode and permissions.</span>}
                    </div>
                  )}
                </section>
                <div className="sidebar-separator" aria-hidden="true" />
                <section className="sidebar-section">
                  <div className="sidebar-section-header">
                    <span>Gallery</span>
                    <span className="sidebar-section-sub">ScreensShot key: {shortcuts.screenshot}</span>
                  </div>
                  <div className="sidebar-row sidebar-row--aligned">
                    <span className="sidebar-label">ScreensShot</span>
                    <button
                      type="button"
                      className="sidebar-button sidebar-screenshot-button"
                      onClick={() => {
                        void captureScreenshot();
                      }}
                      disabled={isSavingScreenshot || !screenshotApiAvailable}
                    >
                      <Camera size={14} />
                      <span>{isSavingScreenshot ? "Capturing..." : "Capture"}</span>
                    </button>
                  </div>
                  <div className="sidebar-gallery-row">
                    <button
                      type="button"
                      className="sidebar-gallery-arrow"
                      onClick={() => scrollGallery("left")}
                      aria-label="Scroll gallery left"
                    >
                      <ChevronLeft size={16} />
                    </button>
                    <div className="sidebar-gallery-strip" ref={galleryStripRef}>
                      {screenshots.map((shot) => (
                        <button
                          key={shot.id}
                          type="button"
                          className="sidebar-gallery-item"
                          onClick={() => setSelectedScreenshotId(shot.id)}
                          title={new Date(shot.createdAtMs).toLocaleString()}
                        >
                          <img src={shot.dataUrl} alt={`Screenshot ${shot.fileName}`} />
                        </button>
                      ))}
                    </div>
                    <button
                      type="button"
                      className="sidebar-gallery-arrow"
                      onClick={() => scrollGallery("right")}
                      aria-label="Scroll gallery right"
                    >
                      <ChevronRight size={16} />
                    </button>
                  </div>
                  {screenshots.length === 0 && (
                    <span className="sidebar-hint">No screenshots yet. Press {shortcuts.screenshot} to capture one.</span>
                  )}
                  {galleryError && <span className="sidebar-hint sidebar-hint--error">{galleryError}</span>}
                </section>
              </>
            )}

            {activeSidebarTab === "shortcuts" && (
              <>
                <div className="sidebar-separator" aria-hidden="true" />
                <section className="sidebar-section">
                  <div className="sidebar-section-header">
                    <span>Shortcut Bindings</span>
                    <span className="sidebar-section-sub">Edit screenshot keybind here</span>
                  </div>
                  <div className="sidebar-row sidebar-row--column">
                    <div className="sidebar-row-top">
                      <span className="sidebar-label">Screenshot Shortcut</span>
                    </div>
                    <input
                      type="text"
                      className={`settings-text-input settings-shortcut-input sidebar-shortcut-input ${screenshotShortcutError ? "error" : ""}`}
                      value={screenshotShortcutInput}
                      onChange={(event) => {
                        const nextValue = event.target.value;
                        setScreenshotShortcutInput(nextValue);
                        setScreenshotShortcutError(getScreenshotShortcutError(nextValue));
                      }}
                      onBlur={() => {
                        const error = getScreenshotShortcutError(screenshotShortcutInput);
                        if (error) {
                          setScreenshotShortcutError(error);
                          return;
                        }
                        const normalized = normalizeShortcut(screenshotShortcutInput.trim());
                        if (!normalized.valid) {
                          setScreenshotShortcutError("Invalid shortcut format.");
                          return;
                        }
                        setScreenshotShortcutError(null);
                        setScreenshotShortcutInput(normalized.canonical);
                        if (normalized.canonical !== shortcuts.screenshot) {
                          onScreenshotShortcutChange(normalized.canonical);
                        }
                      }}
                      onKeyDown={(event) => {
                        if (event.key === "Enter") {
                          (event.target as HTMLInputElement).blur();
                        }
                      }}
                      placeholder="F11"
                      spellCheck={false}
                    />
                  </div>
                  {screenshotShortcutError && <span className="sidebar-hint sidebar-hint--error">{screenshotShortcutError}</span>}
                  <div className="sidebar-row sidebar-row--aligned">
                    <span className="sidebar-label">Toggle Stats</span>
                    <span className="settings-value-badge">{shortcuts.toggleStats}</span>
                  </div>
                  <div className="sidebar-row sidebar-row--aligned">
                    <span className="sidebar-label">Mouse Lock</span>
                    <span className="settings-value-badge">{shortcuts.togglePointerLock}</span>
                  </div>
                  <div className="sidebar-row sidebar-row--aligned">
                    <span className="sidebar-label">Stop Stream</span>
                    <span className="settings-value-badge">{shortcuts.stopStream}</span>
                  </div>
                  {shortcuts.toggleMicrophone && (
                    <div className="sidebar-row sidebar-row--aligned">
                      <span className="sidebar-label">Toggle Microphone</span>
                      <span className="settings-value-badge">{shortcuts.toggleMicrophone}</span>
                    </div>
                  )}
                  <div className="sidebar-row sidebar-row--aligned">
                    <span className="sidebar-label">Toggle Sidebar</span>
                    <span className="settings-value-badge">{isMacClient ? "Cmd+G" : "Ctrl+Shift+G"}</span>
                  </div>
                </section>
              </>
            )}
          </SideBar>
        </>
      )}

      {selectedScreenshot && (
        <div className="sv-shot-modal" role="dialog" aria-modal="true" aria-label="Screenshot preview">
          <button
            type="button"
            className="sv-shot-modal-backdrop"
            onClick={() => setSelectedScreenshotId(null)}
            aria-label="Close screenshot preview"
          />
          <div className="sv-shot-modal-card">
            <div className="sv-shot-modal-head">
              <h4>Screenshot</h4>
              <button
                type="button"
                className="sv-shot-modal-close"
                onClick={() => setSelectedScreenshotId(null)}
                aria-label="Close screenshot preview"
              >
                <X size={16} />
              </button>
            </div>
            <img
              className="sv-shot-modal-image"
              src={selectedScreenshot.dataUrl}
              alt={`Screenshot ${selectedScreenshot.fileName}`}
            />
            <div className="sv-shot-modal-actions">
              <button
                type="button"
                className="sv-shot-modal-btn"
                onClick={() => {
                  void handleSaveScreenshotAs();
                }}
              >
                <Save size={14} />
                <span>Save</span>
              </button>
              <button
                type="button"
                className="sv-shot-modal-btn sv-shot-modal-btn--danger"
                onClick={() => {
                  void handleDeleteScreenshot();
                }}
              >
                <Trash2 size={14} />
                <span>Delete</span>
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Gradient background when no video */}
      {!hasResolution && (
        <div className="sv-empty">
          <div className="sv-empty-grad" />
        </div>
      )}

      {/* Connecting overlay */}
      {isConnecting && (
        <div className="sv-connect">
          <div className="sv-connect-inner">
            <Loader2 className="sv-connect-spin" size={44} />
            <p className="sv-connect-title">Connecting to {gameTitle}</p>
            {PlatformIcon && (
              <div className="sv-connect-platform" title={platformName}>
                <span className="sv-connect-platform-icon">
                  <PlatformIcon />
                </span>
                <span>{platformName}</span>
              </div>
            )}
            <p className="sv-connect-sub">Setting up stream...</p>
          </div>
        </div>
      )}

      {!isConnecting && (
        <div
          className={`sv-session-clock${showSessionClock ? " is-visible" : ""}`}
          title="Current gaming session elapsed time"
          aria-hidden={!showSessionClock}
        >
          <Clock3 size={14} />
          <span>Session {sessionTimeText}</span>
        </div>
      )}

      {streamWarning && !isConnecting && !exitPrompt.open && (
        <div
          className={`sv-time-warning sv-time-warning--${streamWarning.tone}`}
          title="Session time warning"
        >
          <AlertTriangle size={14} />
          <span>
            {streamWarning.message}
            {warningSeconds ? ` · ${warningSeconds} left` : ""}
          </span>
        </div>
      )}

      {/* Stats HUD (top-right) */}
      {showStats && !isConnecting && (
        <div className="sv-stats">
          <div className="sv-stats-head">
            {hasResolution ? (
              <span className="sv-stats-primary">{stats.resolution} · {stats.decodeFps}fps</span>
            ) : (
              <span className="sv-stats-primary sv-stats-wait">Connecting...</span>
            )}
            <span className={`sv-stats-live ${inputLive ? "is-live" : "is-pending"}`}>
              {inputLive ? "Live" : "Sync"}
            </span>
          </div>

          <div className="sv-stats-sub">
            <span className="sv-stats-sub-left">
              {hasCodec ? stats.codec : "N/A"}
              {stats.isHdr && <span className="sv-stats-hdr">HDR</span>}
            </span>
            <span className="sv-stats-sub-right">{bitrateMbps} Mbps</span>
          </div>

          <div className="sv-stats-metrics">
            <span className="sv-stats-chip" title="Round-trip network latency">
              RTT <span className="sv-stats-chip-val" style={{ color: getRttColor(stats.rttMs) }}>{stats.rttMs > 0 ? `${stats.rttMs.toFixed(0)}ms` : "--"}</span>
            </span>
            <span className="sv-stats-chip" title="D = decode time">
              D <span className="sv-stats-chip-val" style={{ color: decodeColor }}>{dText}</span>
            </span>
            <span className="sv-stats-chip" title="R = render time">
              R <span className="sv-stats-chip-val" style={{ color: renderColor }}>{rText}</span>
            </span>
            <span className="sv-stats-chip" title="JB = jitter buffer delay">
              JB <span className="sv-stats-chip-val" style={{ color: jitterBufferColor }}>{jbText}</span>
            </span>
            <span className="sv-stats-chip" title="Packet loss percentage">
              Loss <span className="sv-stats-chip-val" style={{ color: lossColor }}>{stats.packetLossPercent.toFixed(2)}%</span>
            </span>
            <span className="sv-stats-chip" title="Input queue pressure (buffered bytes and delayed flush)">
              IQ <span className="sv-stats-chip-val" style={{ color: inputQueueColor }}>{inputQueueText}</span>
            </span>
          </div>

          <div className="sv-stats-foot">
            Input queue peak {(stats.inputQueuePeakBufferedBytes / 1024).toFixed(1)}KB · drops {stats.inputQueueDropCount} · sched {stats.inputQueueMaxSchedulingDelayMs.toFixed(1)}ms
          </div>

          {(stats.decoderPressureActive || stats.decoderRecoveryAttempts > 0) && (
            <div className="sv-stats-foot">
              Decoder recovery {stats.decoderPressureActive ? "active" : "idle"} · attempts {stats.decoderRecoveryAttempts} · action {stats.decoderRecoveryAction}
            </div>
          )}

          {(stats.gpuType || regionLabel) && (
            <div className="sv-stats-foot">
              {[stats.gpuType, regionLabel].filter(Boolean).join(" · ")}
            </div>
          )}
        </div>
      )}

      {/* Controller indicator (top-left) */}
      {connectedControllers > 0 && !isConnecting && (
        <div className="sv-ctrl" title={`${connectedControllers} controller(s) connected`}>
          <Gamepad2 size={18} />
          {connectedControllers > 1 && <span className="sv-ctrl-n">{connectedControllers}</span>}
        </div>
      )}

      {/* Microphone toggle button (top-left, below controller badge when present) */}
      {showMicIndicator && onToggleMicrophone && (
        <button
          type="button"
          className={`sv-mic${connectedControllers > 0 || antiAfkEnabled ? " sv-mic--stacked" : ""}`}
          onClick={onToggleMicrophone}
          data-enabled={micEnabled}
          title={micEnabled ? "Mute microphone" : "Unmute microphone"}
          aria-label={micEnabled ? "Mute microphone" : "Unmute microphone"}
          aria-pressed={micEnabled}
        >
          {micEnabled ? <Mic size={18} /> : <MicOff size={18} />}
        </button>
      )}

      {/* Anti-AFK indicator (top-left, below controller badge when present) */}
      {antiAfkEnabled && !isConnecting && (
        <div className={`sv-afk${connectedControllers > 0 ? " sv-afk--stacked" : ""}`} title="Anti-AFK is enabled">
          <span className="sv-afk-dot" />
          <span className="sv-afk-label">ANTI-AFK ON</span>
        </div>
      )}

      {/* Hold-Esc release indicator (appears after 1s hold) */}
      {escHoldReleaseIndicator.visible && !isConnecting && (
        <>
          <div className="sv-esc-hold-backdrop" />
          <div className="sv-esc-hold" title="Keep holding Escape to release mouse lock">
            <div className="sv-esc-hold-title">Hold Escape to Release Mouse</div>
            <div className="sv-esc-hold-head">
              <span>Keep holding…</span>
              <span>{escHoldSecondsLeft}s</span>
            </div>
            <div className="sv-esc-hold-track">
              <span className="sv-esc-hold-fill" style={{ transform: `scaleX(${escHoldProgress})` }} />
            </div>
          </div>
        </>
      )}

      {exitPrompt.open && !isConnecting && (
        <div className="sv-exit" role="dialog" aria-modal="true" aria-label="Exit stream confirmation">
          <button
            type="button"
            className="sv-exit-backdrop"
            onClick={onCancelExit}
            aria-label="Cancel exit"
          />
          <div className="sv-exit-card">
            <div className="sv-exit-kicker">Session Control</div>
            <h3 className="sv-exit-title">Exit Stream?</h3>
            <p className="sv-exit-text">
              Do you really want to exit <strong>{exitPrompt.gameTitle}</strong>?
            </p>
            <p className="sv-exit-subtext">Your current cloud gaming session will be closed.</p>
            <div className="sv-exit-actions">
              <button type="button" className="sv-exit-btn sv-exit-btn-cancel" onClick={onCancelExit}>
                Keep Playing
              </button>
              <button type="button" className="sv-exit-btn sv-exit-btn-confirm" onClick={onConfirmExit}>
                Exit Stream
              </button>
            </div>
            <div className="sv-exit-hint">
              <kbd>Enter</kbd> confirm · <kbd>Esc</kbd> cancel
            </div>
          </div>
        </div>
      )}

      {/* Fullscreen toggle */}
      {!hideStreamButtons && (
        <button
          className="sv-fs"
          onClick={handleFullscreenToggle}
          title={isFullscreen ? "Exit fullscreen" : "Enter fullscreen"}
          aria-label={isFullscreen ? "Exit fullscreen" : "Enter fullscreen"}
        >
          {isFullscreen ? <Minimize size={18} /> : <Maximize size={18} />}
        </button>
      )}

      {/* End session button */}
      {!hideStreamButtons && (
        <button
          className="sv-end"
          onClick={onEndSession}
          title="End session"
          aria-label="End session"
        >
          <LogOut size={18} />
        </button>
      )}

      {/* Keyboard hints */}
      {showHints && !isConnecting && (
        <div className="sv-hints">
          <div className="sv-hint"><kbd>{shortcuts.toggleStats}</kbd><span>Stats</span></div>
          <div className="sv-hint"><kbd>{shortcuts.togglePointerLock}</kbd><span>Mouse lock</span></div>
          <div className="sv-hint"><kbd>{shortcuts.stopStream}</kbd><span>Stop</span></div>
          {shortcuts.toggleMicrophone && <div className="sv-hint"><kbd>{shortcuts.toggleMicrophone}</kbd><span>Mic</span></div>}
        </div>
      )}

      {/* Game title (bottom-center, fades) */}
      {hasResolution && showHints && (
        <div className="sv-title-bar">
          <span className="sv-title-game">{gameTitle}</span>
          {PlatformIcon && (
            <span className="sv-title-platform" title={platformName}>
              <span className="sv-title-platform-icon">
                <PlatformIcon />
              </span>
              <span>{platformName}</span>
            </span>
          )}
        </div>
      )}
    </div>
  );
}
