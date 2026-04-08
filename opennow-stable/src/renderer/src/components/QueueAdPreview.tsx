import { AlertTriangle, Loader2, PauseCircle, PlayCircle, RefreshCcw, XCircle } from "lucide-react";
import { useEffect, useRef, useState, type JSX } from "react";

type QueueAdPlaybackState = "loading" | "playing" | "paused" | "stalled" | "blocked" | "timeout" | "error";
type QueueAdPlaybackEvent = "playing" | "paused" | "ended";

interface QueueAdPreviewProps {
  mediaUrl: string;
  title?: string;
  onPlaybackEvent?: (event: QueueAdPlaybackEvent) => void;
}

interface PlaybackPresentation {
  label: string;
  message: string;
  retryLabel?: string;
  icon: typeof Loader2;
}

const INITIAL_PLAY_TIMEOUT_MS = 8000;
const STALL_THRESHOLD_MS = 4000;
const STALL_CHECK_INTERVAL_MS = 1000;

function getPlaybackPresentation(state: QueueAdPlaybackState): PlaybackPresentation {
  switch (state) {
    case "playing":
      return {
        label: "Playing",
        message: "",
        icon: PlayCircle,
      };
    case "paused":
      return {
        label: "Paused",
        message: "Ad paused before completion.",
        retryLabel: "Resume",
        icon: PauseCircle,
      };
    case "stalled":
      return {
        label: "Stalled",
        message: "Playback stopped progressing.",
        retryLabel: "Retry",
        icon: AlertTriangle,
      };
    case "blocked":
      return {
        label: "Autoplay blocked",
        message: "Browser blocked automatic playback.",
        retryLabel: "Start",
        icon: AlertTriangle,
      };
    case "timeout":
      return {
        label: "Timed out",
        message: "Ad did not start in time.",
        retryLabel: "Retry",
        icon: AlertTriangle,
      };
    case "error":
      return {
        label: "Playback error",
        message: "Media failed to load.",
        retryLabel: "Retry",
        icon: XCircle,
      };
    case "loading":
    default:
      return {
        label: "Loading",
        message: "Preparing playback…",
        icon: Loader2,
      };
  }
}

export function QueueAdPreview({ mediaUrl, title, onPlaybackEvent }: QueueAdPreviewProps): JSX.Element {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const playbackStateRef = useRef<QueueAdPlaybackState>("loading");
  const progressRef = useRef({ currentTime: 0, unchangedMs: 0 });
  // Guards against firing "ended" twice when the proactive timeupdate path
  // already fired it before the native ended event arrives.
  const finishFiredRef = useRef(false);
  // Store callback in a ref so the setup effect never depends on its identity.
  // Inline arrow functions passed by callers change reference on every render;
  // without this, the effect would tear down and restart the video on every
  // queue-position update.
  const onPlaybackEventRef = useRef(onPlaybackEvent);
  useEffect(() => {
    onPlaybackEventRef.current = onPlaybackEvent;
  });
  const [playbackState, setPlaybackState] = useState<QueueAdPlaybackState>("loading");

  const setPlayback = (next: QueueAdPlaybackState): void => {
    playbackStateRef.current = next;
    setPlaybackState(next);
  };

  const attemptPlayback = async (): Promise<void> => {
    const video = videoRef.current;
    if (!video) {
      return;
    }

    setPlayback("loading");

    // Try audible playback first (matching official client behaviour).
    // Fall back to muted if the autoplay policy blocks audio.
    try {
      video.muted = false;
      await video.play();
      return;
    } catch {
      // Unmuted autoplay blocked — retry muted
    }

    try {
      video.muted = true;
      await video.play();
    } catch (error) {
      if (error instanceof DOMException && error.name === "NotAllowedError") {
        setPlayback("blocked");
        return;
      }
      console.warn("Queue ad playback failed:", error);
      setPlayback("error");
    }
  };

  useEffect(() => {
    setPlayback("loading");
    progressRef.current = { currentTime: 0, unchangedMs: 0 };
    finishFiredRef.current = false;
  }, [mediaUrl]);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) {
      return;
    }

    let disposed = false;
    let startupTimeoutId = 0;

    const clearStartupTimeout = (): void => {
      if (startupTimeoutId) {
        window.clearTimeout(startupTimeoutId);
        startupTimeoutId = 0;
      }
    };

    const handlePlaying = (): void => {
      clearStartupTimeout();
      progressRef.current = { currentTime: video.currentTime, unchangedMs: 0 };
      setPlayback("playing");
      onPlaybackEventRef.current?.("playing");
    };

    const handlePause = (): void => {
      if (!video.ended && playbackStateRef.current === "playing") {
        setPlayback("paused");
        onPlaybackEventRef.current?.("paused");
      }
    };

      const handleTimeUpdate = (): void => {
        if (finishFiredRef.current) {
          return;
        }
        const d = video.duration;
        if (isFinite(d) && d > 0 && video.currentTime >= d) {
          finishFiredRef.current = true;
          clearStartupTimeout();
          onPlaybackEventRef.current?.("ended");
        }
      };

    const handleEnded = (): void => {
      clearStartupTimeout();
      // Only fire if the proactive timeupdate path hasn't already done so.
      if (!finishFiredRef.current) {
        finishFiredRef.current = true;
        onPlaybackEventRef.current?.("ended");
      }
    };

    const handleWaiting = (): void => {
      if (!video.paused && !video.ended) {
        setPlayback("stalled");
      }
    };

    const handleStalled = (): void => {
      if (!video.paused && !video.ended) {
        setPlayback("stalled");
      }
    };

    const handleError = (): void => {
      clearStartupTimeout();
      setPlayback("error");
    };

    video.addEventListener("playing", handlePlaying);
    video.addEventListener("pause", handlePause);
    video.addEventListener("timeupdate", handleTimeUpdate);
    video.addEventListener("ended", handleEnded);
    video.addEventListener("waiting", handleWaiting);
    video.addEventListener("stalled", handleStalled);
    video.addEventListener("error", handleError);

    startupTimeoutId = window.setTimeout(() => {
      if (!disposed && playbackStateRef.current !== "playing") {
        setPlayback("timeout");
      }
    }, INITIAL_PLAY_TIMEOUT_MS);

    const stallIntervalId = window.setInterval(() => {
      if (disposed || video.paused || video.ended || video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA) {
        return;
      }

      const progress = progressRef.current;
      const delta = Math.abs(video.currentTime - progress.currentTime);
      if (delta < 0.05) {
        progress.unchangedMs += STALL_CHECK_INTERVAL_MS;
      } else {
        progress.unchangedMs = 0;
      }
      progress.currentTime = video.currentTime;

      if (progress.unchangedMs >= STALL_THRESHOLD_MS && playbackStateRef.current === "playing") {
        setPlayback("stalled");
      }
    }, STALL_CHECK_INTERVAL_MS);

    void attemptPlayback();

    return () => {
      disposed = true;
      clearStartupTimeout();
      window.clearInterval(stallIntervalId);
      video.removeEventListener("playing", handlePlaying);
      video.removeEventListener("pause", handlePause);
      video.removeEventListener("timeupdate", handleTimeUpdate);
      video.removeEventListener("ended", handleEnded);
      video.removeEventListener("waiting", handleWaiting);
      video.removeEventListener("stalled", handleStalled);
      video.removeEventListener("error", handleError);
    };
  }, [mediaUrl]); // intentionally excludes onPlaybackEvent — stored in ref above

  const presentation = getPlaybackPresentation(playbackState);
  const StatusIcon = presentation.icon;
  const showFrameOverlay = playbackState !== "playing";

  return (
    <div className={`queue-ad-preview queue-ad-preview--${playbackState}`}>
      <div className="queue-ad-preview-frame">
        <video
          ref={videoRef}
          className="queue-ad-preview-video"
          src={mediaUrl}
          autoPlay
          playsInline
          preload="auto"
          aria-label={title ? `${title} advertisement` : "Advertisement"}
        />
        {showFrameOverlay && (
          <div className="queue-ad-preview-overlay" aria-hidden="true">
            <div className="queue-ad-preview-overlay-inner">
              <StatusIcon className={`queue-ad-preview-overlay-icon${playbackState === "loading" ? " queue-ad-preview-icon--spinning" : ""}`} size={18} />
              <span className="queue-ad-preview-overlay-title">{title ?? "Advertisement"}</span>
            </div>
          </div>
        )}
      </div>
      <div className="queue-ad-preview-status" aria-live="polite">
        <div className="queue-ad-preview-status-main">
          <StatusIcon className={`queue-ad-preview-icon${playbackState === "loading" ? " queue-ad-preview-icon--spinning" : ""}`} size={16} />
          <div className="queue-ad-preview-copy">
            <span className="queue-ad-preview-label">{presentation.label}</span>
            {presentation.message && <span className="queue-ad-preview-message">{presentation.message}</span>}
          </div>
        </div>
        {presentation.retryLabel && (
          <button className="queue-ad-preview-retry" onClick={() => void attemptPlayback()} type="button">
            <RefreshCcw size={14} />
            <span>{presentation.retryLabel}</span>
          </button>
        )}
      </div>
    </div>
  );
}