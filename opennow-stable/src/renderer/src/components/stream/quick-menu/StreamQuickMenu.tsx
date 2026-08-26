import { useRef } from "react";
import type { Dispatch, JSX, RefObject, SetStateAction } from "react";
import { AnimatePresence, m } from "motion/react";
import { Bug, Gauge, Images, Keyboard, LogOut, Save, SlidersHorizontal, Trash2, X } from "lucide-react";
import type {
  FrameInterpolationSettings,
  MicrophoneMode,
  RecordingFps,
  RecordingResolution,
  SubscriptionInfo,
  VideoShaderSettings,
} from "@shared/gfn";
import SideBar from "../../SideBar";
import type { StreamDiagnosticsStore } from "../../../utils/streamDiagnosticsStore";
import { useMicMeter } from "../../../hooks/useMicMeter";
import type { useScreenshotGallery } from "../../../hooks/useScreenshotGallery";
import type { useStreamRecorder } from "../../../hooks/useStreamRecorder";
import type { StreamMenuTab } from "../../../hooks/useStreamMenuNavigation";
import { StreamQuickMenuControlsPage } from "./StreamQuickMenuControlsPage";
import { StreamQuickMenuMediaPage } from "./StreamQuickMenuMediaPage";
import { StreamQuickMenuSessionPage } from "./StreamQuickMenuSessionPage";
import {
  StreamQuickMenuShortcutsPage,
  type StreamShortcutBindings,
  useStreamQuickMenuShortcuts,
} from "./StreamQuickMenuShortcutsPage";

interface StreamQuickMenuProps {
  open: boolean;
  onClose: () => void;
  sidebarRef: RefObject<HTMLElement | null>;
  activeTab: StreamMenuTab;
  setActiveTab: Dispatch<SetStateAction<StreamMenuTab>>;
  onEndSession: () => void;
  onReportBug: () => void;
  gameTitle: string;
  platformName: string;
  PlatformIcon: (() => JSX.Element) | null;
  subscriptionInfo: SubscriptionInfo | null;
  sessionStartedAtMs: number | null;
  isStreaming: boolean;
  sessionTimeRemainingText: string | null;
  isFullscreen: boolean;
  isPointerLocked: boolean;
  onToggleFullscreen: () => void;
  onTogglePointerLock: () => void;
  onToggleMicrophone?: () => void;
  showSessionTimeRemainingInStatsOverlay: boolean;
  onShowSessionTimeRemainingInStatsOverlayChange: (value: boolean) => void;
  sidebarToggleShortcutDisplay: string;
  controllerSidebarShortcutDisplay: string;
  mouseSensitivity: number;
  onMouseSensitivityChange: (value: number) => void;
  mouseAcceleration: number;
  onMouseAccelerationChange: (value: number) => void;
  nativeStreamingEnabled: boolean;
  videoShader: VideoShaderSettings;
  onVideoShaderChange: (value: VideoShaderSettings) => void;
  frameInterpolation: FrameInterpolationSettings;
  onFrameInterpolationChange: (value: FrameInterpolationSettings) => void;
  microphoneMode: MicrophoneMode;
  onMicrophoneModeChange: (value: MicrophoneMode) => void;
  diagnosticsStore: StreamDiagnosticsStore;
  micTrack: MediaStreamTrack | null;
  shortcuts: StreamShortcutBindings;
  isMacClient: boolean;
  onScreenshotShortcutChange: (value: string) => void;
  onRecordingShortcutChange: (value: string) => void;
  screenshotGallery: ReturnType<typeof useScreenshotGallery>;
  streamRecorder: ReturnType<typeof useStreamRecorder>;
  recordingBitrateMbps: number | null;
  recordingResolution: RecordingResolution;
  recordingFps: RecordingFps;
  onRecordingResolutionChange: (value: RecordingResolution) => void;
  onRecordingFpsChange: (value: RecordingFps) => void;
  onRecordingBitrateMbpsChange: (value: number | null) => void;
}

export function StreamQuickMenu({
  open,
  onClose,
  sidebarRef,
  activeTab,
  setActiveTab,
  onEndSession,
  onReportBug,
  gameTitle,
  platformName,
  PlatformIcon,
  subscriptionInfo,
  sessionStartedAtMs,
  isStreaming,
  sessionTimeRemainingText,
  isFullscreen,
  isPointerLocked,
  onToggleFullscreen,
  onTogglePointerLock,
  onToggleMicrophone,
  showSessionTimeRemainingInStatsOverlay,
  onShowSessionTimeRemainingInStatsOverlayChange,
  sidebarToggleShortcutDisplay,
  controllerSidebarShortcutDisplay,
  mouseSensitivity,
  onMouseSensitivityChange,
  mouseAcceleration,
  onMouseAccelerationChange,
  nativeStreamingEnabled,
  videoShader,
  onVideoShaderChange,
  frameInterpolation,
  onFrameInterpolationChange,
  microphoneMode,
  onMicrophoneModeChange,
  diagnosticsStore,
  micTrack,
  shortcuts,
  isMacClient,
  onScreenshotShortcutChange,
  onRecordingShortcutChange,
  screenshotGallery,
  streamRecorder,
  recordingBitrateMbps,
  recordingResolution,
  recordingFps,
  onRecordingResolutionChange,
  onRecordingFpsChange,
  onRecordingBitrateMbpsChange,
}: StreamQuickMenuProps): JSX.Element {
  const micMeterRef = useRef<HTMLCanvasElement | null>(null);
  useMicMeter(micMeterRef, micTrack, open && microphoneMode !== "disabled");
  const shortcutEditor = useStreamQuickMenuShortcuts({
    shortcuts,
    isMacClient,
    onScreenshotShortcutChange,
    onRecordingShortcutChange,
  });

  return (
    <>
      <AnimatePresence>
        {open && (
          <m.div
            key="quick-menu-backdrop"
            className="sv-sidebar-backdrop"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.18 }}
            onMouseDown={(event) => event.stopPropagation()}
            onClick={onClose}
          />
        )}
      </AnimatePresence>
      <AnimatePresence>
        {open && (
          <SideBar
            key="quick-menu-sidebar"
            title="Quick menu"
            className="sv-sidebar"
            elementRef={sidebarRef}
            onClose={onClose}
            footer={(
              <>
                <div className="sidebar-controller-hints" aria-hidden="true">
                  <span><kbd>A</kbd> Select</span>
                  <span><kbd>B</kbd> Back</span>
                  <span><kbd>LB</kbd><kbd>RB</kbd> Pages</span>
                </div>
                <button
                  type="button"
                  className="sidebar-report-bug-button"
                  onClick={onReportBug}
                >
                  <Bug size={16} />
                  <span>Report a stream bug</span>
                </button>
                <button
                  type="button"
                  className="sidebar-exit-session-button"
                  onClick={onEndSession}
                >
                  <LogOut size={16} />
                  <span>End session</span>
                </button>
              </>
            )}
          >
            <div className="sidebar-tabs" role="tablist" aria-label="Quick menu pages">
              <button
                type="button"
                role="tab"
                aria-selected={activeTab === "session"}
                className={`sidebar-tab${activeTab === "session" ? " sidebar-tab--active" : ""}`}
                onClick={() => setActiveTab("session")}
              >
                <Gauge size={16} />
                <span>Session</span>
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={activeTab === "controls"}
                className={`sidebar-tab${activeTab === "controls" ? " sidebar-tab--active" : ""}`}
                onClick={() => setActiveTab("controls")}
              >
                <SlidersHorizontal size={16} />
                <span>Controls</span>
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={activeTab === "media"}
                className={`sidebar-tab${activeTab === "media" ? " sidebar-tab--active" : ""}`}
                onClick={() => setActiveTab("media")}
              >
                <Images size={16} />
                <span>Media</span>
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={activeTab === "shortcuts"}
                className={`sidebar-tab${activeTab === "shortcuts" ? " sidebar-tab--active" : ""}`}
                onClick={() => setActiveTab("shortcuts")}
              >
                <Keyboard size={16} />
                <span>Keys</span>
              </button>
            </div>

            {activeTab === "session" && (
              <StreamQuickMenuSessionPage
                gameTitle={gameTitle}
                platformName={platformName}
                PlatformIcon={PlatformIcon}
                subscriptionInfo={subscriptionInfo}
                sessionStartedAtMs={sessionStartedAtMs}
                isStreaming={isStreaming}
                sessionTimeRemainingText={sessionTimeRemainingText}
                isFullscreen={isFullscreen}
                isPointerLocked={isPointerLocked}
                onToggleFullscreen={onToggleFullscreen}
                onTogglePointerLock={onTogglePointerLock}
                onToggleMicrophone={onToggleMicrophone}
                onCaptureScreenshot={() => { void screenshotGallery.captureScreenshot(); }}
                isSavingScreenshot={screenshotGallery.isSavingScreenshot}
                screenshotApiAvailable={screenshotGallery.screenshotApiAvailable}
                showSessionTimeRemainingInStatsOverlay={showSessionTimeRemainingInStatsOverlay}
                onShowSessionTimeRemainingInStatsOverlayChange={onShowSessionTimeRemainingInStatsOverlayChange}
                sidebarToggleShortcutDisplay={sidebarToggleShortcutDisplay}
                controllerSidebarShortcutDisplay={controllerSidebarShortcutDisplay}
              />
            )}

            {activeTab === "controls" && (
              <StreamQuickMenuControlsPage
                mouseSensitivity={mouseSensitivity}
                onMouseSensitivityChange={onMouseSensitivityChange}
                mouseAcceleration={mouseAcceleration}
                onMouseAccelerationChange={onMouseAccelerationChange}
                nativeStreamingEnabled={nativeStreamingEnabled}
                videoShader={videoShader}
                onVideoShaderChange={onVideoShaderChange}
                frameInterpolation={frameInterpolation}
                onFrameInterpolationChange={onFrameInterpolationChange}
                microphoneMode={microphoneMode}
                onMicrophoneModeChange={onMicrophoneModeChange}
                diagnosticsStore={diagnosticsStore}
                micTrack={micTrack}
                micMeterRef={micMeterRef}
              />
            )}

            {activeTab === "media" && (
              <StreamQuickMenuMediaPage
                screenshotShortcut={shortcuts.screenshot}
                screenshots={screenshotGallery.screenshots}
                isSavingScreenshot={screenshotGallery.isSavingScreenshot}
                screenshotApiAvailable={screenshotGallery.screenshotApiAvailable}
                galleryError={screenshotGallery.galleryError}
                galleryStripRef={screenshotGallery.galleryStripRef}
                onCaptureScreenshot={() => { void screenshotGallery.captureScreenshot(); }}
                onSelectScreenshot={screenshotGallery.setSelectedScreenshotId}
                onScrollGallery={screenshotGallery.scrollGallery}
                recordingShortcut={shortcuts.recording}
                recordings={streamRecorder.recordings}
                isRecording={streamRecorder.isRecording}
                recordingDurationMs={streamRecorder.recordingDurationMs}
                recordingError={streamRecorder.recordingError}
                recordingApiAvailable={streamRecorder.recordingApiAvailable}
                usedMimeType={streamRecorder.usedMimeType}
                recordingStatus={streamRecorder.recordingStatus}
                recordingBitrateMbps={recordingBitrateMbps}
                recordingResolution={recordingResolution}
                recordingFps={recordingFps}
                onRecordingResolutionChange={onRecordingResolutionChange}
                onRecordingFpsChange={onRecordingFpsChange}
                onRecordingBitrateMbpsChange={onRecordingBitrateMbpsChange}
                recCarouselRef={streamRecorder.recCarouselRef}
                onToggleRecording={() => { void streamRecorder.toggleRecording(); }}
                onDeleteRecording={(id) => { void streamRecorder.deleteRecording(id); }}
                onScrollRecordings={streamRecorder.scrollRecordings}
              />
            )}

            {activeTab === "shortcuts" && (
              <StreamQuickMenuShortcutsPage
                shortcuts={shortcuts}
                isMacClient={isMacClient}
                sidebarToggleShortcutDisplay={sidebarToggleShortcutDisplay}
                controllerSidebarShortcutDisplay={controllerSidebarShortcutDisplay}
                onScreenshotShortcutChange={onScreenshotShortcutChange}
                onRecordingShortcutChange={onRecordingShortcutChange}
                editor={shortcutEditor}
              />
            )}
          </SideBar>
        )}
      </AnimatePresence>

      {screenshotGallery.selectedScreenshot && (
        <div className="sv-shot-modal" role="dialog" aria-modal="true" aria-label="Screenshot preview">
          <button
            type="button"
            className="sv-shot-modal-backdrop"
            onClick={() => screenshotGallery.setSelectedScreenshotId(null)}
            aria-label="Close screenshot preview"
          />
          <div className="sv-shot-modal-card">
            <div className="sv-shot-modal-head">
              <h4>Screenshot</h4>
              <button
                type="button"
                className="sv-shot-modal-close"
                onClick={() => screenshotGallery.setSelectedScreenshotId(null)}
                aria-label="Close screenshot preview"
              >
                <X size={16} />
              </button>
            </div>
            <img
              className="sv-shot-modal-image"
              src={screenshotGallery.selectedScreenshot.dataUrl}
              alt={`Screenshot ${screenshotGallery.selectedScreenshot.fileName}`}
            />
            <div className="sv-shot-modal-actions">
              <button
                type="button"
                className="sv-shot-modal-btn"
                onClick={() => { void screenshotGallery.saveSelectedScreenshotAs(); }}
              >
                <Save size={14} />
                <span>Save</span>
              </button>
              <button
                type="button"
                className="sv-shot-modal-btn sv-shot-modal-btn--danger"
                onClick={() => { void screenshotGallery.deleteSelectedScreenshot(); }}
              >
                <Trash2 size={14} />
                <span>Delete</span>
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
