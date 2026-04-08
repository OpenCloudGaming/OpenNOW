import { Loader2, Monitor, Cpu, Wifi, X, XCircle } from "lucide-react";
import type { JSX } from "react";
import type { SessionAdState } from "@shared/gfn";
import { getStoreDisplayName, getStoreIconComponent } from "./GameCard";
import { QueueAdPreview } from "./QueueAdPreview";

export interface StreamLoadingProps {
  gameTitle: string;
  gameCover?: string;
  platformStore?: string;
  status: "queue" | "setup" | "starting" | "connecting";
  queuePosition?: number;
  estimatedWait?: string;
  adState?: SessionAdState;
  activeAdMediaUrl?: string;
  error?: {
    title: string;
    description: string;
    code?: string;
  };
  onAdPlaybackEvent?: (event: "playing" | "paused" | "ended", adId: string) => void;
  onCancel: () => void;
}

const steps = [
  { id: "queue", label: "Queue", icon: Monitor },
  { id: "setup", label: "Setup", icon: Cpu },
  { id: "ready", label: "Ready", icon: Wifi },
] as const;

function getStatusMessage(
  status: StreamLoadingProps["status"],
  queuePosition?: number,
  adState?: SessionAdState,
  isError = false,
): string {
  if (isError) {
    return "Game launch failed";
  }
  if (adState?.isQueuePaused) {
    return "Session queue paused";
  }
  switch (status) {
    case "queue":
      return queuePosition ? `Position #${queuePosition} in queue` : "Waiting in queue...";
    case "setup":
      return "Setting up your gaming rig...";
    case "starting":
      return "Starting stream...";
    case "connecting":
      return "Connecting to server...";
    default:
      return "Loading...";
  }
}

function getActiveStepIndex(status: StreamLoadingProps["status"]): number {
  switch (status) {
    case "queue":
      return 0;
    case "setup":
      return 1;
    case "starting":
    case "connecting":
      return 2;
    default:
      return 0;
  }
}

function getAdSummary(adState?: SessionAdState): string | null {
  if (!adState?.isAdsRequired) {
    return null;
  }
  if (adState.message) {
    return adState.message;
  }
  if (adState.isQueuePaused) {
    return "Resume ads to stay in queue.";
  }
  return adState.ads.length > 0
    ? `${adState.ads.length} ad${adState.ads.length === 1 ? "" : "s"} available for queue progression.`
    : "Ad playback is required while your session is queued.";
}

export function StreamLoading({
  gameTitle,
  gameCover,
  platformStore,
  status,
  queuePosition,
  estimatedWait,
  adState,
  activeAdMediaUrl,
  error,
  onAdPlaybackEvent,
  onCancel,
}: StreamLoadingProps): JSX.Element {
  const hasError = Boolean(error);
  const activeStepIndex = getActiveStepIndex(status);
  const statusMessage = getStatusMessage(status, queuePosition, adState, hasError);
  const platformName = platformStore ? getStoreDisplayName(platformStore) : "";
  const PlatformIcon = platformStore ? getStoreIconComponent(platformStore) : null;
  const adSummary = getAdSummary(adState);
  const activeAd = adState?.ads[0];
  const cachedAdMediaUrl = activeAdMediaUrl ?? activeAd?.mediaUrl;
  const activeAdDurationSeconds = activeAd?.durationMs ? Math.round(activeAd.durationMs / 1000) : undefined;

  return (
    <div className={`sload${hasError ? " sload--error" : ""}`}>
      <div className="sload-backdrop" />

      {/* Animated accent glow behind content */}
      <div className="sload-glow" />

      <div className="sload-content">
        {/* Game Info Header */}
        <div className="sload-game">
          <div className="sload-cover">
            {gameCover ? (
              <img src={gameCover} alt={gameTitle} className="sload-cover-img" />
            ) : (
              <div className="sload-cover-empty">
                <Monitor size={28} />
              </div>
            )}
            <div className="sload-cover-shine" />
          </div>
          <div className="sload-game-meta">
            <span className="sload-label">{hasError ? "Launch Error" : "Now Loading"}</span>
            <h2 className="sload-title" title={gameTitle}>
              {gameTitle}
            </h2>
            {PlatformIcon && (
              <div className="sload-platform" title={platformName}>
                <span className="sload-platform-icon">
                  <PlatformIcon />
                </span>
                <span>{platformName}</span>
              </div>
            )}
          </div>
        </div>

        {/* Progress Steps */}
        <div className="sload-steps">
          {steps.map((step, index) => {
            const StepIcon = step.icon;
            const isFailed = hasError && index === activeStepIndex;
            const isActive = !isFailed && index === activeStepIndex;
            const isCompleted = index < activeStepIndex;
            const isPending = index > activeStepIndex;
            const nextIsFailed = hasError && index + 1 === activeStepIndex;

            return (
              <div
                key={step.id}
                className={`sload-step${isActive ? " active" : ""}${isCompleted ? " completed" : ""}${isPending ? " pending" : ""}${isFailed ? " failed" : ""}`}
              >
                <div className="sload-step-dot">
                  {isFailed ? <X size={18} /> : <StepIcon size={18} />}
                </div>
                <span className="sload-step-name">{step.label}</span>
                {index < steps.length - 1 && (
                  <div className={`sload-step-line${nextIsFailed ? " failed" : ""}`}>
                    <div className="sload-step-line-fill" />
                  </div>
                )}
              </div>
            );
          })}
        </div>

        {/* Status Display */}
        <div className={`sload-status${hasError ? " sload-status--error" : ""}`}>
          {hasError ? <XCircle size={28} className="sload-error-icon" /> : <Loader2 size={28} className="sload-spin" />}
          <div className="sload-status-text">
            <p className="sload-message">{statusMessage}</p>
            {!hasError && activeAd && cachedAdMediaUrl && (
              <div className={`sload-ad${adState?.isQueuePaused ? " sload-ad--paused" : ""}`}>
                <div className="sload-ad-copy">
                  <span className="sload-ad-chip">Ad Queue</span>
                  <p className="sload-ad-title">{activeAd.title ?? "Advertisement in progress"}</p>
                  {adSummary && <p className="sload-ad-message">{adSummary}</p>}
                  <div className="sload-ad-meta">
                    {activeAdDurationSeconds && <span>{activeAdDurationSeconds}s spot</span>}
                    {adState?.gracePeriodSeconds && <span>{adState.gracePeriodSeconds}s grace window</span>}
                  </div>
                </div>
                <div className="sload-ad-media">
                  <QueueAdPreview
                    mediaUrl={cachedAdMediaUrl}
                    title={activeAd.title}
                    onPlaybackEvent={(event) => onAdPlaybackEvent?.(event, activeAd.adId)}
                  />
                </div>
              </div>
            )}
            {hasError && error && (
              <>
                <p className="sload-error-title">{error.title}</p>
                <p className="sload-error-desc">{error.description}</p>
                {error.code && <p className="sload-error-code">{error.code}</p>}
              </>
            )}
            {status === "queue" && estimatedWait && (
              <p className="sload-queue">
                <span className="sload-wait">~{estimatedWait}</span>
              </p>
            )}
          </div>
        </div>

        {/* Cancel */}
        <button className="sload-cancel" onClick={onCancel} aria-label="Cancel loading">
          <X size={16} />
          <span>{hasError ? "Close" : "Cancel"}</span>
        </button>
      </div>
    </div>
  );
}
