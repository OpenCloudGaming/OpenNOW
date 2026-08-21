import type { JSX, RefObject } from "react";
import type {
  FrameInterpolationSettings,
  MicrophoneMode,
  VideoShaderSettings,
} from "@shared/gfn";
import {
  DEFAULT_VIDEO_SHADER_SETTINGS,
  FRAME_INTERPOLATION_FACTOR_OPTIONS,
  FRAME_INTERPOLATION_QUALITY_OPTIONS,
} from "@shared/gfn";
import type { StreamDiagnosticsStore } from "../../../utils/streamDiagnosticsStore";
import { SidebarMicMutedBadge } from "../StreamEmptyStates";

const MICROPHONE_MODES = [
  { value: "disabled" as MicrophoneMode, label: "Disabled", description: "No microphone input" },
  { value: "push-to-talk" as MicrophoneMode, label: "Push-to-Talk", description: "Hold a key to talk" },
  { value: "voice-activity" as MicrophoneMode, label: "Voice Activity", description: "Always listen" },
];

const VIDEO_FILTER_CONTROLS = [
  { key: "sharpen", label: "Sharpen", min: 0, max: 100, neutral: 0, format: (value: number) => `${value}%`, hint: "Contrast-adaptive sharpening. Counters stream compression blur." },
  { key: "saturation", label: "Saturation", min: 0, max: 200, neutral: 100, format: (value: number) => `${value}%` },
  { key: "contrast", label: "Contrast", min: 50, max: 150, neutral: 100, format: (value: number) => `${value}%` },
  { key: "brightness", label: "Brightness", min: 50, max: 150, neutral: 100, format: (value: number) => `${value}%` },
  { key: "vibrance", label: "Vibrance", min: 0, max: 100, neutral: 0, format: (value: number) => `${value}%`, hint: "Boosts muted colors without oversaturating." },
  { key: "filmGrain", label: "Film Grain", min: 0, max: 100, neutral: 0, format: (value: number) => `${value}%` },
] as const;

interface StreamQuickMenuControlsPageProps {
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
  micMeterRef: RefObject<HTMLCanvasElement | null>;
}

export function StreamQuickMenuControlsPage({
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
  micMeterRef,
}: StreamQuickMenuControlsPageProps): JSX.Element {
  return (
    <div className="sidebar-page" role="tabpanel">
      <section className="sidebar-section">
        <div className="sidebar-section-header">
          <span>Mouse Preferences</span>
          <span className="sidebar-section-sub">Fine-tune cursor movement.</span>
        </div>
        <div className="sidebar-row sidebar-row--column">
          <div className="sidebar-row-top">
            <span className="sidebar-label">Mouse Sensitivity</span>
            <span className="settings-value-badge">{mouseSensitivity.toFixed(2)}x</span>
          </div>
          <input
            type="range"
            name="mouse-sensitivity"
            aria-label="Mouse sensitivity"
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
            name="mouse-acceleration"
            aria-label="Mouse accelerator"
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
          <span>Frame Interpolation</span>
          <span className="sidebar-section-sub">Experimental Framegen WebGPU processing.</span>
        </div>
        {nativeStreamingEnabled ? (
          <span className="sidebar-hint">
            Frame interpolation is unavailable while the native streamer renders the video.
          </span>
        ) : (
          <>
            <div className="sidebar-row sidebar-row--aligned">
              <span className="sidebar-label">Enable Interpolation</span>
              <label className="sidebar-mini-toggle" title="Enable neural frame interpolation" tabIndex={0}>
                <input
                  type="checkbox"
                  name="enable-frame-interpolation"
                  checked={frameInterpolation.enabled}
                  aria-label="Enable frame interpolation"
                  onChange={(event) => onFrameInterpolationChange({
                    ...frameInterpolation,
                    enabled: event.target.checked,
                  })}
                />
                <span className="sidebar-mini-toggle-track" />
              </label>
            </div>
            {frameInterpolation.enabled && (
              <>
                <div className="sidebar-row sidebar-row--column">
                  <div className="sidebar-row-top">
                    <span className="sidebar-label">Frame Factor</span>
                    <span className="settings-value-badge">{frameInterpolation.factor}×</span>
                  </div>
                  <div className="sidebar-chip-row" role="group" aria-label="Frame interpolation factor">
                    {FRAME_INTERPOLATION_FACTOR_OPTIONS.map((factor) => (
                      <button
                        key={factor}
                        type="button"
                        className={`sidebar-chip${frameInterpolation.factor === factor ? " sidebar-chip--active" : ""}`}
                        aria-pressed={frameInterpolation.factor === factor}
                        onClick={() => onFrameInterpolationChange({ ...frameInterpolation, factor })}
                      >
                        <span>{factor}×</span>
                      </button>
                    ))}
                  </div>
                </div>
                <div className="sidebar-row sidebar-row--column">
                  <div className="sidebar-row-top">
                    <span className="sidebar-label">Model Quality</span>
                    <span className="settings-value-badge">{frameInterpolation.quality}p</span>
                  </div>
                  <div className="sidebar-chip-row" role="group" aria-label="Frame interpolation model quality">
                    {FRAME_INTERPOLATION_QUALITY_OPTIONS.map((quality) => (
                      <button
                        key={quality}
                        type="button"
                        className={`sidebar-chip${frameInterpolation.quality === quality ? " sidebar-chip--active" : ""}`}
                        aria-pressed={frameInterpolation.quality === quality}
                        onClick={() => onFrameInterpolationChange({ ...frameInterpolation, quality })}
                      >
                        <span>{quality}p</span>
                      </button>
                    ))}
                  </div>
                  <span className="sidebar-hint">
                    Lower quality reduces GPU load. Changes restart the interpolation runtime.
                  </span>
                </div>
              </>
            )}
          </>
        )}
      </section>
      <div className="sidebar-separator" aria-hidden="true" />
      <section className="sidebar-section">
        <div className="sidebar-section-header">
          <span>Video Filters</span>
          <span className="sidebar-section-sub">GPU shaders applied to the stream.</span>
        </div>
        {nativeStreamingEnabled ? (
          <span className="sidebar-hint">Video filters are unavailable while the native streamer renders the video.</span>
        ) : (
          <>
            <div className="sidebar-row sidebar-row--aligned">
              <span className="sidebar-label">Enable Filters</span>
              <label className="sidebar-mini-toggle" title="Enable GPU post-processing filters" tabIndex={0}>
                <input
                  type="checkbox"
                  name="enable-video-filters"
                  checked={videoShader.enabled}
                  aria-label="Enable video filters"
                  onChange={(event) => onVideoShaderChange({ ...videoShader, enabled: event.target.checked })}
                />
                <span className="sidebar-mini-toggle-track" />
              </label>
            </div>
            {videoShader.enabled && (
              <>
                {VIDEO_FILTER_CONTROLS.map((control) => (
                  <div key={control.key} className="sidebar-row sidebar-row--column">
                    <div className="sidebar-row-top">
                      <span className="sidebar-label">{control.label}</span>
                      <span className="settings-value-badge">{control.format(videoShader[control.key])}</span>
                    </div>
                    <input
                      type="range"
                      name={`video-filter-${control.key}`}
                      aria-label={`${control.label} video filter`}
                      className="settings-slider"
                      min={control.min}
                      max={control.max}
                      step={1}
                      value={videoShader[control.key]}
                      onChange={(event) => {
                        const next = Number(event.target.value);
                        if (Number.isFinite(next)) {
                          onVideoShaderChange({
                            ...videoShader,
                            [control.key]: Math.max(control.min, Math.min(control.max, Math.round(next))),
                          });
                        }
                      }}
                      onDoubleClick={() => onVideoShaderChange({ ...videoShader, [control.key]: control.neutral })}
                    />
                    {"hint" in control && control.hint && <span className="sidebar-hint">{control.hint}</span>}
                  </div>
                ))}
                <div className="sidebar-row sidebar-row--aligned">
                  <span className="sidebar-label">Reset Filters</span>
                  <button
                    type="button"
                    className="sidebar-button"
                    onClick={() => onVideoShaderChange({ ...DEFAULT_VIDEO_SHADER_SETTINGS, enabled: true })}
                  >
                    <span>Reset</span>
                  </button>
                </div>
              </>
            )}
          </>
        )}
      </section>
      <div className="sidebar-separator" aria-hidden="true" />
      <section className="sidebar-section">
        <div className="sidebar-section-header">
          <span>Audio</span>
          <span className="sidebar-section-sub">Configure microphone handling.</span>
        </div>
        <div className="sidebar-row sidebar-row--column">
          <div className="sidebar-row-top">
            <span className="sidebar-label">Microphone Mode</span>
            <span className="settings-value-badge">
              {MICROPHONE_MODES.find((option) => option.value === microphoneMode)?.label ?? microphoneMode}
            </span>
          </div>
          <div className="sidebar-chip-row">
            {MICROPHONE_MODES.map((option) => (
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
            {MICROPHONE_MODES.find((option) => option.value === microphoneMode)?.description ?? ""}
          </span>
        </div>
        {microphoneMode !== "disabled" && (
          <div className="sidebar-row sidebar-row--column">
            <div className="sidebar-row-top">
              <span className="sidebar-label">Send level</span>
              <SidebarMicMutedBadge diagnosticsStore={diagnosticsStore} micTrack={micTrack} />
            </div>
            <canvas
              ref={micMeterRef}
              className="mic-meter-canvas"
              aria-label="Microphone send level (what others hear)"
            />
            {!micTrack && <span className="sidebar-hint">Mic not active — check mode and permissions.</span>}
          </div>
        )}
      </section>
    </div>
  );
}
