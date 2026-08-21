import { AlertTriangle, CheckCircle2, Cpu, ExternalLink, Keyboard, Monitor, RefreshCcw, XCircle } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type JSX } from "react";
import type { NativeStreamerStatus, NativeVideoBackendCapability, Settings } from "@shared/gfn";
import {
  createUnsupportedNativeStreamerStatus,
  isNativeExternalRendererSupported,
  isNativeStreamerSupportedPlatform,
  NATIVE_STREAMER_UNSUPPORTED_PLATFORM_MESSAGE,
} from "@shared/gfn";
import { useTranslation } from "../../../i18n";
import {
  formatNativeRuntimeLabel,
  formatNativeVideoCodec,
  formatNativeVideoBackendName,
  getAvailableNativeCodecLabels,
  getNativeRuntimeBadgeClass,
  nativeVideoBackendOptions,
} from "../settingsFormatters";
import { MotionSpinner } from "../../MotionSpinner";
import { ModalSurface } from "../../ui/ModalSurface";

const nativePlatformHint = `${navigator.platform} ${navigator.userAgent}`;
const isNativeStreamerPlatform = isNativeStreamerSupportedPlatform(nativePlatformHint);
const supportsNativeExternalRenderer = isNativeExternalRendererSupported(nativePlatformHint);

function getNativeHostPlatform(): "windows" | "macos" | "linux" | "other" {
  const normalized = nativePlatformHint.toLowerCase();
  if (normalized.includes("win")) return "windows";
  if (normalized.includes("mac")) return "macos";
  if (normalized.includes("linux")) return "linux";
  return "other";
}

function getHostVideoBackends(status: NativeStreamerStatus | null): NativeVideoBackendCapability[] {
  const hostPlatform = getNativeHostPlatform();
  return (status?.videoBackends ?? []).filter(
    (backend) => backend.platform === hostPlatform || backend.platform === "cross-platform",
  );
}

export interface SettingsNativeStreamerSectionProps {
  settings: Settings;
  showAll: boolean;
  showSection?: boolean;
  handleChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  onBlockingOverlayChange?: (blocking: boolean) => void;
}

export function SettingsNativeStreamerSection({
  settings,
  showAll,
  showSection = true,
  handleChange,
  onBlockingOverlayChange,
}: SettingsNativeStreamerSectionProps): JSX.Element {
  const { t } = useTranslation();
  const [nativeStreamerStatus, setNativeStreamerStatus] = useState<NativeStreamerStatus | null>(null);
  const [nativeStreamerStatusLoading, setNativeStreamerStatusLoading] = useState(false);
  const [nativeStreamerEnablePromptOpen, setNativeStreamerEnablePromptOpen] = useState(false);
  const nativeStreamerEnablePromptConfirmRef = useRef<HTMLButtonElement | null>(null);
  const hostVideoBackends = getHostVideoBackends(nativeStreamerStatus);
  const selectableVideoBackendOptions = nativeVideoBackendOptions.filter(
    (option) => option.value === "auto"
      || hostVideoBackends.some((backend) => backend.backend === option.value),
  );
  const readyHardwareBackendCount = hostVideoBackends.filter(
    (backend) => backend.available && backend.backend !== "software",
  ).length;
  const activeVideoBackendId = nativeStreamerStatus?.activeVideoBackend?.backend;
  const primaryStatusMessage = nativeStreamerStatus?.message.trim() ?? "";
  const runtimeStatusMessage = nativeStreamerStatus?.runtime.message.trim() ?? "";
  const distinctRuntimeStatusMessage = runtimeStatusMessage && runtimeStatusMessage !== primaryStatusMessage
    ? runtimeStatusMessage
    : "";
  const runtimePath = nativeStreamerStatus?.runtime.path?.trim() ?? "";

  const refreshNativeStreamerStatus = useCallback(async () => {
    if (!isNativeStreamerPlatform) {
      setNativeStreamerStatus(createUnsupportedNativeStreamerStatus());
      setNativeStreamerStatusLoading(false);
      return;
    }

    setNativeStreamerStatusLoading(true);
    try {
      setNativeStreamerStatus(await window.openNow.getNativeStreamerStatus());
    } catch (error) {
      console.warn("[Settings] Failed to detect native streamer:", error);
      setNativeStreamerStatus({
        detected: false,
        available: false,
        supportsOfferAnswer: false,
        runtime: {
          source: "unknown",
          selfContained: false,
          message: "Native runtime could not be checked.",
        },
        message: "Native streamer status could not be checked.",
      });
    } finally {
      setNativeStreamerStatusLoading(false);
    }
  }, []);

  useEffect(() => {
    void refreshNativeStreamerStatus();
  }, [refreshNativeStreamerStatus]);

  useEffect(() => {
    if (nativeStreamerEnablePromptOpen) {
      onBlockingOverlayChange?.(true);
    }
  }, [nativeStreamerEnablePromptOpen, onBlockingOverlayChange]);

  useEffect(() => () => onBlockingOverlayChange?.(false), [onBlockingOverlayChange]);

  const openNativeStreamerEnablePrompt = useCallback((): void => {
    setNativeStreamerEnablePromptOpen(true);
  }, []);

  const closeNativeStreamerEnablePrompt = useCallback((): void => {
    setNativeStreamerEnablePromptOpen(false);
  }, []);

  const handleNativeStreamerEnablePromptExit = useCallback((): void => {
    onBlockingOverlayChange?.(false);
  }, [onBlockingOverlayChange]);

  const confirmNativeStreamerEnablePrompt = useCallback((): void => {
    handleChange("streamClientMode", "native");
    closeNativeStreamerEnablePrompt();
  }, [closeNativeStreamerEnablePrompt, handleChange]);

  const handleNativeStreamerToggleChange = useCallback((checked: boolean): void => {
    if (!checked) {
      handleChange("streamClientMode", "web");
      return;
    }

    if (settings.streamClientMode === "native") {
      return;
    }

    openNativeStreamerEnablePrompt();
  }, [handleChange, openNativeStreamerEnablePrompt, settings.streamClientMode]);

  const setNativeFramePacing = useCallback((mode: "low-latency" | "smooth") => {
    if (mode === "low-latency") {
      handleChange("enableCloudGsync", false);
      handleChange("nativeCloudGsyncMode", "disabled");
      handleChange("nativeD3dFullscreenMode", "disabled");
      return;
    }

    handleChange("enableCloudGsync", true);
    handleChange("nativeCloudGsyncMode", "auto");
    handleChange("nativeD3dFullscreenMode", "auto");
  }, [handleChange]);

  return (
    <>
      {showSection && (
      <section className="settings-section">
        {showAll && <div className="settings-section-context">{t("settings.sections.nativeStreamer")}</div>}
        <div className="settings-section-header">
          <Cpu />
          <h2>{t("settings.nativeStreamer.title")}</h2>
        </div>
        <div className="settings-rows settings-rows--grouped">
          {!isNativeStreamerPlatform ? (
            <div className="settings-row settings-row--complex">
              <div className="settings-row-top settings-row-top--compact">
                <label className="settings-label settings-label--wrap">
                  <span className="settings-label-title">
                    {t("settings.nativeStreamer.nativeStreaming")}
                    <span className="settings-inline-badge settings-inline-badge--beta">{t("app.labels.experimental")}</span>
                  </span>
                </label>
              </div>
              <span className="settings-input-hint">
                {NATIVE_STREAMER_UNSUPPORTED_PLATFORM_MESSAGE}
              </span>
            </div>
          ) : (
            <>
              <div className="settings-group">
                <div className="settings-group-header">
                  <h3>{t("settings.nativeStreamer.runtime")}</h3>
                  <p>{t("settings.nativeStreamer.runtimeDescription")}</p>
                </div>
                <div className="settings-group-rows">
              <div className="settings-row settings-row--toggle">
                <div className="settings-row-top settings-row-top--compact">
                  <label className="settings-label settings-label--wrap" htmlFor="settings-native-streaming-enabled">
                    <span className="settings-label-title">
                      {t("settings.nativeStreamer.nativeStreaming")}
                      <span className="settings-inline-badge settings-inline-badge--beta">{t("app.labels.experimental")}</span>
                    </span>
                  </label>
                  <label className="settings-toggle">
                    <input
                      id="settings-native-streaming-enabled"
                      type="checkbox"
                      checked={settings.streamClientMode === "native"}
                      onChange={(e) => handleNativeStreamerToggleChange(e.target.checked)}
                    />
                    <span className="settings-toggle-track" />
                  </label>
                </div>
                <span className="settings-subtle-hint">
                  {t("settings.nativeStreamer.nativeStreamingHint")}
                </span>
                <div className="settings-native-report-links">
                  <a href="https://github.com/OpenCloudGaming/OpenNOW/issues" target="_blank" rel="noreferrer">
                    <span>{t("settings.nativeStreamer.reportOnGithubIssues")}</span>
                    <ExternalLink size={12} />
                  </a>
                  <a href="https://discord.gg/8EJYaJcNfD" target="_blank" rel="noreferrer">
                    <span>{t("settings.nativeStreamer.reportOnDiscord")}</span>
                    <ExternalLink size={12} />
                  </a>
                </div>
              </div>

              {settings.streamClientMode === "native" && (
                <div className="settings-row settings-row--choice">
                  <label className="settings-label">{t("settings.nativeStreamer.transportMode")}</label>
                  <div className="settings-chip-row">
                    <button
                      type="button"
                      className={`settings-chip ${settings.transportMode === "webrtc" ? "active" : ""}`}
                      aria-pressed={settings.transportMode === "webrtc"}
                      onClick={() => handleChange("transportMode", "webrtc")}
                    >
                      <span>{t("settings.nativeStreamer.transportModeWebrtc")}</span>
                    </button>
                    <button
                      type="button"
                      className={`settings-chip ${settings.transportMode === "nvst" ? "active" : ""}`}
                      aria-pressed={settings.transportMode === "nvst"}
                      onClick={() => handleChange("transportMode", "nvst")}
                    >
                      <span>{t("settings.nativeStreamer.transportModeNvst")}</span>
                    </button>
                  </div>
                  <span className="settings-subtle-hint">
                    {t("settings.nativeStreamer.transportModeHint")}
                  </span>
                </div>
              )}

              <div className="settings-row settings-row--toggle">
                <div className="settings-row-top settings-row-top--compact">
                  <label className="settings-label settings-label--wrap" htmlFor="settings-native-show-stats">
                    <span className="settings-label-title">{t("settings.nativeStreamer.showNativeStreamerStats")}</span>
                  </label>
                  <label className="settings-toggle">
                    <input
                      id="settings-native-show-stats"
                      type="checkbox"
                      checked={settings.showNativeStreamerStats}
                      onChange={(e) => handleChange("showNativeStreamerStats", e.target.checked)}
                    />
                    <span className="settings-toggle-track" />
                  </label>
                </div>
                <span className="settings-subtle-hint">
                  {t("settings.nativeStreamer.showNativeStreamerStatsHint")}
                </span>
              </div>

              <div className="settings-row settings-row--status-action">
                <div className="settings-row-top settings-row-top--compact">
                  <label className="settings-label settings-label--wrap">
                    <span className="settings-label-title">{t("settings.nativeStreamer.streamerStatus")}</span>
                  </label>
                  <button
                    type="button"
                    className="settings-icon-button"
                    onClick={() => void refreshNativeStreamerStatus()}
                    disabled={nativeStreamerStatusLoading}
                    title={t("settings.nativeStreamer.checkNativeStreamer")}
                    aria-label={t("settings.nativeStreamer.checkNativeStreamer")}
                  >
                    {nativeStreamerStatusLoading ? <MotionSpinner size={15} label="Checking streamer status" /> : <RefreshCcw size={15} />}
                  </button>
                </div>
                <div className="settings-chip-row">
                  <span
                    className={`settings-inline-badge ${
                      nativeStreamerStatusLoading
                        ? "settings-inline-badge--codec-testing"
                        : nativeStreamerStatus?.available
                          ? "settings-inline-badge--codec-gpu"
                          : "settings-inline-badge--updater-error"
                    }`}
                  >
                    {nativeStreamerStatusLoading
                      ? t("app.status.checking")
                      : nativeStreamerStatus?.available
                        ? t("settings.nativeStreamer.nativeRuntimeReady")
                        : t("settings.nativeStreamer.notReady")}
                  </span>
                  <span className={`settings-inline-badge ${getNativeRuntimeBadgeClass(nativeStreamerStatus)}`}>
                    {formatNativeRuntimeLabel(nativeStreamerStatus)}
                  </span>
                </div>
                <span className="settings-subtle-hint">
                  {primaryStatusMessage || t("settings.nativeStreamer.statusDefaultHint")}
                </span>
                {distinctRuntimeStatusMessage ? (
                  <span className="settings-subtle-hint">{distinctRuntimeStatusMessage}</span>
                ) : null}
                {runtimePath ? (
                  <span className="settings-native-runtime-path">
                    <strong>
                      {nativeStreamerStatus?.runtime.selfContained
                        ? t("settings.nativeStreamer.nativeExecutablePath")
                        : t("settings.nativeStreamer.nativeRuntime")}
                    </strong>
                    <code>{runtimePath}</code>
                  </span>
                ) : null}
              </div>

                </div>
              </div>

              <div className="settings-group">
                <div className="settings-group-header">
                  <h3>{t("settings.nativeStreamer.videoOutput")}</h3>
                  <p>{t("settings.nativeStreamer.videoOutputDescription")}</p>
                </div>
                <div className="settings-group-rows">
              <div className="settings-row settings-row--complex settings-native-capability-row">
                <div className="settings-native-capability-header">
                  <div>
                    <span className="settings-native-capability-kicker">{t("settings.nativeStreamer.thisPc")}</span>
                    <label className="settings-label">{t("settings.nativeStreamer.supportedVideoBackends")}</label>
                  </div>
                  <span className={`settings-native-capability-count ${readyHardwareBackendCount > 0 ? "is-ready" : ""}`}>
                    {t("settings.nativeStreamer.hardwarePathsReady", { count: readyHardwareBackendCount })}
                  </span>
                </div>

                {hostVideoBackends.length > 0 ? (
                  <div className="settings-native-capability-grid">
                    {hostVideoBackends.map((backend) => {
                      const isActive = backend.backend === activeVideoBackendId;
                      const availableCodecs = getAvailableNativeCodecLabels(backend);
                      return (
                        <article
                          key={backend.backend}
                          className={`settings-native-capability-card ${backend.available ? "is-supported" : "is-unsupported"} ${isActive ? "is-active" : ""}`}
                        >
                          <div className="settings-native-capability-card-top">
                            <span className="settings-native-capability-icon" aria-hidden="true">
                              {backend.available ? <CheckCircle2 size={16} /> : <XCircle size={16} />}
                            </span>
                            <div className="settings-native-capability-name">
                              <strong>{formatNativeVideoBackendName(backend.backend)}</strong>
                              <span>{backend.available ? t("settings.nativeStreamer.supported") : t("settings.nativeStreamer.unavailable")}</span>
                            </div>
                            {isActive ? (
                              <span className="settings-inline-badge settings-inline-badge--codec-gpu">
                                {settings.nativeVideoBackend === "auto"
                                  ? t("settings.nativeStreamer.autoPick")
                                  : t("settings.nativeStreamer.selected")}
                              </span>
                            ) : null}
                          </div>

                          <div className="settings-native-capability-codecs" aria-label={t("settings.nativeStreamer.codecSupport")}>
                            {backend.codecs.map((codec) => (
                              <span
                                key={codec.codec}
                                className={codec.available ? "is-supported" : "is-unsupported"}
                                title={codec.available ? codec.decoder : codec.reason}
                              >
                                {formatNativeVideoCodec(codec.codec)}
                              </span>
                            ))}
                          </div>

                          <div className="settings-native-capability-meta">
                            <span>{backend.sink ?? t("settings.nativeStreamer.noRenderer")}</span>
                            <span>
                              {backend.zeroCopyModes.length > 0
                                ? backend.zeroCopyModes.join(" · ")
                                : t("settings.nativeStreamer.systemMemory")}
                            </span>
                          </div>

                          {!backend.available && backend.reason ? (
                            <p className="settings-native-capability-reason">{backend.reason}</p>
                          ) : availableCodecs.length === 0 ? (
                            <p className="settings-native-capability-reason">{t("settings.nativeStreamer.noCodecsAvailable")}</p>
                          ) : null}
                        </article>
                      );
                    })}
                  </div>
                ) : (
                  <span className="settings-subtle-hint">
                    {nativeStreamerStatusLoading
                      ? t("settings.nativeStreamer.probingBackends")
                      : t("settings.nativeStreamer.videoPathDefaultHint")}
                  </span>
                )}

                {(!nativeStreamerStatus?.available || hostVideoBackends.length === 0) && (
                  <span className="settings-subtle-hint">
                    {nativeStreamerStatus?.activeVideoBackend?.reason
                      ?? t("settings.nativeStreamer.capabilityProbeHint")}
                  </span>
                )}
              </div>

              {isNativeStreamerPlatform && selectableVideoBackendOptions.length > 1 && <div className="settings-row settings-row--choice">
                <label className="settings-label">{t("settings.nativeStreamer.directxBackend")}</label>
                <div className="settings-chip-row">
                  {selectableVideoBackendOptions.map((option) => {
                    const capability = option.value === "auto"
                      ? undefined
                      : hostVideoBackends.find((backend) => backend.backend === option.value);
                    const unavailable = option.value !== "auto"
                      && nativeStreamerStatus?.detected === true
                      && capability?.available !== true;
                    return (
                      <button
                        key={option.value}
                        type="button"
                        className={`settings-chip ${settings.nativeVideoBackend === option.value ? "active" : ""}`}
                        aria-pressed={settings.nativeVideoBackend === option.value}
                        onClick={() => handleChange("nativeVideoBackend", option.value)}
                        title={unavailable ? capability?.reason ?? t("settings.nativeStreamer.backendUnavailableOnPc") : option.description}
                        disabled={unavailable}
                      >
                        <span>{option.label}</span>
                        {option.value !== "auto" && capability ? (
                          <span className={`settings-backend-support-dot ${capability.available ? "is-supported" : "is-unsupported"}`} aria-hidden="true" />
                        ) : null}
                      </button>
                    );
                  })}
                </div>
                <span className="settings-subtle-hint">
                  {t("settings.nativeStreamer.directxBackendHint")}
                </span>
              </div>}

              <div className="settings-row settings-row--choice">
                <label className="settings-label">{t("settings.nativeStreamer.framePacing")}</label>
                <div className="settings-chip-row">
                  <button
                    type="button"
                    className={`settings-chip ${!settings.enableCloudGsync ? "active" : ""}`}
                    aria-pressed={!settings.enableCloudGsync}
                    onClick={() => setNativeFramePacing("low-latency")}
                  >
                    <span>{t("settings.nativeStreamer.lowestLatency")}</span>
                  </button>
                  <button
                    type="button"
                    className={`settings-chip ${settings.enableCloudGsync ? "active" : ""}`}
                    aria-pressed={settings.enableCloudGsync}
                    onClick={() => setNativeFramePacing("smooth")}
                  >
                    <span>{t("settings.nativeStreamer.smoothGsync")}</span>
                  </button>
                </div>
                <span className="settings-subtle-hint">
                  {t("settings.nativeStreamer.framePacingHint")}
                </span>
              </div>

              {supportsNativeExternalRenderer ? (
                <div className="settings-row settings-row--choice">
                  <label className="settings-label">{t("settings.nativeStreamer.renderMode")}</label>
                  <div className="settings-chip-row">
                    <button type="button" className={`settings-chip ${!settings.nativeExternalRenderer ? "active" : ""}`} aria-pressed={!settings.nativeExternalRenderer} onClick={() => handleChange("nativeExternalRenderer", false)}>
                      <span>{t("settings.nativeStreamer.renderModeInternal")}</span>
                    </button>
                    <button type="button" className={`settings-chip ${settings.nativeExternalRenderer ? "active" : ""}`} aria-pressed={settings.nativeExternalRenderer} onClick={() => handleChange("nativeExternalRenderer", true)}>
                      <span>{t("settings.nativeStreamer.renderModeExternal")}</span>
                    </button>
                  </div>
                  <span className="settings-subtle-hint">{t("settings.nativeStreamer.renderModeHint")}</span>
                </div>
              ) : (
                <div className="settings-row settings-row--choice">
                  <label className="settings-label">{t("settings.nativeStreamer.renderMode")}</label>
                  <div className="settings-chip-row"><span className="settings-inline-badge">{t("settings.nativeStreamer.renderModeInternal")}</span></div>
                  <span className="settings-subtle-hint">{t("settings.nativeStreamer.renderModeInternalOnlyHint")}</span>
                </div>
              )}

                </div>
              </div>
            </>
          )}
        </div>
      </section>
      )}
      <ModalSurface
        open={nativeStreamerEnablePromptOpen}
        onClose={closeNativeStreamerEnablePrompt}
        onExitComplete={handleNativeStreamerEnablePromptExit}
        motion="compact"
        overlayClassName="native-streamer-warning"
        backdropClassName="native-streamer-warning-backdrop"
        panelClassName="native-streamer-warning-card"
        ariaLabelledBy="native-streamer-warning-title"
        ariaDescribedBy="native-streamer-warning-copy"
        backdropLabel={t("app.actions.cancel")}
        initialFocusRef={nativeStreamerEnablePromptConfirmRef}
      >
            <div className="native-streamer-warning-kicker">
              <AlertTriangle size={14} />
              {t("settings.nativeStreamer.enablePromptKicker")}
            </div>
            <h3 id="native-streamer-warning-title" className="native-streamer-warning-title">
              {t("settings.nativeStreamer.enablePromptTitle")}
            </h3>
            <p id="native-streamer-warning-copy" className="native-streamer-warning-text">
              {t("settings.nativeStreamer.enablePromptBody")}
            </p>

            <div className="native-streamer-warning-list">
              <div className="native-streamer-warning-list-item">
                <Cpu size={16} />
                <span>{t("settings.nativeStreamer.enablePromptNewSessions")}</span>
              </div>
              <div className="native-streamer-warning-list-item">
                <Keyboard size={16} />
                <span>{t("settings.nativeStreamer.enablePromptShortcuts")}</span>
              </div>
              <div className="native-streamer-warning-list-item">
                <Monitor size={16} />
                <span>{t("settings.nativeStreamer.enablePromptAltTab")}</span>
              </div>
            </div>

            <div className="native-streamer-warning-actions">
              <button
                type="button"
                className="native-streamer-warning-btn native-streamer-warning-btn--secondary"
                onClick={closeNativeStreamerEnablePrompt}
              >
                {t("settings.nativeStreamer.enablePromptCancel")}
              </button>
              <button
                type="button"
                className="native-streamer-warning-btn native-streamer-warning-btn--primary"
                onClick={confirmNativeStreamerEnablePrompt}
                ref={nativeStreamerEnablePromptConfirmRef}
              >
                {t("settings.nativeStreamer.enablePromptEnable")}
              </button>
            </div>
            <div className="native-streamer-warning-hint">
              <kbd>Esc</kbd> {t("settings.nativeStreamer.enablePromptEsc")}
            </div>
      </ModalSurface>
    </>
  );
}
