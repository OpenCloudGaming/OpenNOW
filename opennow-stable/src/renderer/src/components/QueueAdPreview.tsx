import { AlertTriangle, Loader2, PauseCircle, PlayCircle, RefreshCcw, XCircle } from "lucide-react";
import { forwardRef, useEffect, useImperativeHandle, useRef, useState, type JSX } from "react";

type QueueAdPlaybackState = "loading" | "playing" | "paused" | "stalled" | "blocked" | "timeout" | "error";
export type QueueAdPlaybackEvent = "loadstart" | "playing" | "paused" | "ended" | "timeupdate" | "error";

export interface QueueAdPreviewHandle {
  attemptPlayback: () => Promise<void>;
  pause: () => void;
  resume: () => Promise<void>;
  getSnapshot: () => {
    currentTime: number;
    paused: boolean;
    ended: boolean;
    readyState: number;
    muted: boolean;
  } | null;
}

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
        label: "",
        message: "",
        icon: Loader2,
      };
  }
}

export const QueueAdPreview = forwardRef<QueueAdPreviewHandle, QueueAdPreviewProps>(function QueueAdPreview(
  { mediaUrl, title, onPlaybackEvent }: QueueAdPreviewProps,
  ref,
): JSX.Element {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const playbackStateRef = useRef<QueueAdPlaybackState>("loading");
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

  useImperativeHandle(ref, () => ({
    attemptPlayback,
    pause: () => {
      videoRef.current?.pause();
    },
    resume: async () => {
      await attemptPlayback();
    },
    getSnapshot: () => {
      const video = videoRef.current;
      if (!video) {
        return null;
      }
      return {
        currentTime: video.currentTime,
        paused: video.paused,
        ended: video.ended,
        readyState: video.readyState,
        muted: video.muted,
      };
    },
  }));

  useEffect(() => {
    setPlayback("loading");
    finishFiredRef.current = false;
  }, [mediaUrl]);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) {
      return;
    }

    const originalVolume = video.volume;
    const restoreOriginalVolume = (): void => {
      try {
        video.volume = originalVolume;
      } catch {
        // ignore
      }
    };

    try {
      video.volume = Math.max(0, Math.min(1, originalVolume * 0.3125));
    } catch {
      // Ignore if setting volume is not permitted
    }

    const handlePlaying = (): void => {
      setPlayback("playing");
      onPlaybackEventRef.current?.("playing");
    };

    const handleLoadStart = (): void => {
      finishFiredRef.current = false;
      setPlayback("loading");
      onPlaybackEventRef.current?.("loadstart");
    };

    const handlePause = (): void => {
      if (!video.ended && playbackStateRef.current === "playing") {
        setPlayback("paused");
        onPlaybackEventRef.current?.("paused");
      }
    };

    const handleTimeUpdate = (): void => {
      onPlaybackEventRef.current?.("timeupdate");
    };

    const handleEnded = (): void => {
      // Only fire if the proactive timeupdate path hasn't already done so.
      if (!finishFiredRef.current) {
        finishFiredRef.current = true;
        onPlaybackEventRef.current?.("ended");
      }

      restoreOriginalVolume();
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
      setPlayback("error");
      onPlaybackEventRef.current?.("error");
      restoreOriginalVolume();
    };

    video.addEventListener("loadstart", handleLoadStart);
    video.addEventListener("playing", handlePlaying);
    video.addEventListener("pause", handlePause);
    video.addEventListener("timeupdate", handleTimeUpdate);
    video.addEventListener("ended", handleEnded);
    video.addEventListener("waiting", handleWaiting);
    video.addEventListener("stalled", handleStalled);
    video.addEventListener("error", handleError);

    void attemptPlayback();

    return () => {
      video.removeEventListener("loadstart", handleLoadStart);
      video.removeEventListener("playing", handlePlaying);
      video.removeEventListener("pause", handlePause);
      video.removeEventListener("timeupdate", handleTimeUpdate);
      video.removeEventListener("ended", handleEnded);
      video.removeEventListener("waiting", handleWaiting);
      video.removeEventListener("stalled", handleStalled);
      video.removeEventListener("error", handleError);
      restoreOriginalVolume();
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
            </div>
          </div>
        )}
      </div>
      {presentation.retryLabel && (
        <div className="queue-ad-preview-status" aria-live="polite">
          <div className="queue-ad-preview-status-main">
            <StatusIcon className="queue-ad-preview-icon" size={16} />
            <div className="queue-ad-preview-copy">
              <span className="queue-ad-preview-label">{presentation.label}</span>
              {presentation.message && <span className="queue-ad-preview-message">{presentation.message}</span>}
            </div>
          </div>
          <button className="queue-ad-preview-retry" onClick={() => void attemptPlayback()} type="button">
            <RefreshCcw size={14} />
            <span>{presentation.retryLabel}</span>
          </button>
        </div>
      )}
    </div>
  );
});