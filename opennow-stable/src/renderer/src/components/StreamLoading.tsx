import { Loader2, Monitor, Cpu, Wifi, X } from "lucide-react";
import type { JSX } from "react";

export interface StreamLoadingProps {
  gameTitle: string;
  gameCover?: string;
  status: "queue" | "setup" | "starting" | "connecting";
  queuePosition?: number;
  estimatedWait?: string;
  onCancel: () => void;
}

const steps = [
  { id: "queue", label: "Queue", icon: Monitor },
  { id: "setup", label: "Setup", icon: Cpu },
  { id: "ready", label: "Ready", icon: Wifi },
] as const;

function getStatusMessage(status: StreamLoadingProps["status"], queuePosition?: number): string {
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

export function StreamLoading({
  gameTitle,
  gameCover,
  status,
  queuePosition,
  estimatedWait,
  onCancel,
}: StreamLoadingProps): JSX.Element {
  const activeStepIndex = getActiveStepIndex(status);
  const statusMessage = getStatusMessage(status, queuePosition);

  return (
    <div className="sload">
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
            <span className="sload-label">Now Loading</span>
            <h2 className="sload-title" title={gameTitle}>
              {gameTitle}
            </h2>
          </div>
        </div>

        {/* Progress Steps */}
        <div className="sload-steps">
          {steps.map((step, index) => {
            const StepIcon = step.icon;
            const isActive = index === activeStepIndex;
            const isCompleted = index < activeStepIndex;
            const isPending = index > activeStepIndex;

            return (
              <div
                key={step.id}
                className={`sload-step${isActive ? " active" : ""}${isCompleted ? " completed" : ""}${isPending ? " pending" : ""}`}
              >
                <div className="sload-step-dot">
                  <StepIcon size={18} />
                </div>
                <span className="sload-step-name">{step.label}</span>
                {index < steps.length - 1 && (
                  <div className="sload-step-line">
                    <div className="sload-step-line-fill" />
                  </div>
                )}
              </div>
            );
          })}
        </div>

        {/* Status Display */}
        <div className="sload-status">
          <Loader2 size={28} className="sload-spin" />
          <div className="sload-status-text">
            <p className="sload-message">{statusMessage}</p>
            {status === "queue" && queuePosition !== undefined && queuePosition > 0 && (
              <p className="sload-queue">
                Position <span className="sload-queue-num">#{queuePosition}</span>
                {estimatedWait && <span className="sload-wait"> · ~{estimatedWait}</span>}
              </p>
            )}
          </div>
        </div>

        {/* Cancel */}
        <button className="sload-cancel" onClick={onCancel} aria-label="Cancel loading">
          <X size={16} />
          <span>Cancel</span>
        </button>
      </div>
    </div>
  );
}
