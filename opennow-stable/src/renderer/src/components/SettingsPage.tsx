import { Monitor, Volume2, Mouse, Settings2, Globe, Save, Check, Search, X, Loader } from "lucide-react";
import { useState, useCallback, useMemo, useEffect } from "react";

import type { StreamRegion, VideoCodec, ColorQuality, EntitledResolution } from "@shared/gfn";
import { colorQualityRequiresHevc } from "@shared/gfn";

interface Settings {
  resolution: string;
  fps: number;
  codec: VideoCodec;
  colorQuality: ColorQuality;
  maxBitrateMbps: number;
  region: string;
  clipboardPaste: boolean;
  mouseSensitivity?: number;
}

interface SettingsPageProps {
  settings: Settings;
  regions: StreamRegion[];
  onSettingChange: (key: string, value: unknown) => void;
}

const codecOptions: VideoCodec[] = ["H264", "H265", "AV1"];

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

/* ── Validation ───────────────────────────────────────────────────── */

function isValidResolution(value: string): boolean {
  return /^\d{3,5}x\d{3,5}$/.test(value);
}

/* ── Component ────────────────────────────────────────────────────── */

export function SettingsPage({ settings, regions, onSettingChange }: SettingsPageProps): JSX.Element {
  const [savedIndicator, setSavedIndicator] = useState(false);
  const [regionSearch, setRegionSearch] = useState("");
  const [regionDropdownOpen, setRegionDropdownOpen] = useState(false);
  const [resolutionInput, setResolutionInput] = useState(settings.resolution);
  const [fpsInput, setFpsInput] = useState(String(settings.fps));
  const [resolutionError, setResolutionError] = useState(false);
  const [fpsError, setFpsError] = useState(false);

  // Dynamic entitled resolutions from MES API
  const [entitledResolutions, setEntitledResolutions] = useState<EntitledResolution[]>([]);
  const [subscriptionLoading, setSubscriptionLoading] = useState(true);

  // Fetch subscription data on mount
  useEffect(() => {
    let cancelled = false;

    async function load(): Promise<void> {
      try {
        const session = await window.openNow.getAuthSession();
        if (!session || cancelled) {
          setSubscriptionLoading(false);
          return;
        }

        const sub = await window.openNow.fetchSubscription({
          userId: session.user.userId,
        });

        if (!cancelled && sub.entitledResolutions.length > 0) {
          setEntitledResolutions(sub.entitledResolutions);
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
    (key: string, value: unknown) => {
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

  const handleResolutionBlur = (): void => {
    const val = resolutionInput.trim();
    if (isValidResolution(val)) {
      setResolutionError(false);
      if (val !== settings.resolution) {
        handleChange("resolution", val);
      }
    } else {
      setResolutionError(true);
    }
  };

  const handleResolutionKeyDown = (e: React.KeyboardEvent): void => {
    if (e.key === "Enter") {
      (e.target as HTMLInputElement).blur();
    }
  };

  const handleFpsBlur = (): void => {
    const n = parseInt(fpsInput, 10);
    if (!isNaN(n) && n >= 1 && n <= 360) {
      setFpsError(false);
      if (n !== settings.fps) {
        handleChange("fps", n);
      }
    } else {
      setFpsError(true);
    }
  };

  const handleFpsKeyDown = (e: React.KeyboardEvent): void => {
    if (e.key === "Enter") {
      (e.target as HTMLInputElement).blur();
    }
  };

  return (
    <div className="settings-page">
      <header className="settings-header">
        <Settings2 size={22} />
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
            <Monitor size={18} />
            <h2>Video</h2>
          </div>

          <div className="settings-rows">
            {/* Resolution — dynamic text input */}
            <div className="settings-row">
              <label className="settings-label">Resolution</label>
              <div className="settings-input-group">
                <input
                  type="text"
                  className={`settings-text-input ${resolutionError ? "error" : ""}`}
                  value={resolutionInput}
                  onChange={(e) => setResolutionInput(e.target.value)}
                  onBlur={handleResolutionBlur}
                  onKeyDown={handleResolutionKeyDown}
                  placeholder="1920x1080"
                  spellCheck={false}
                />
                {resolutionError && <span className="settings-input-hint">Format: WIDTHxHEIGHT</span>}
              </div>
            </div>

            {/* Resolution presets — dynamic or static */}
            <div className="settings-row settings-row--column">
              <label className="settings-label">
                Presets
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
                              setResolutionInput(res.value);
                              setResolutionError(false);
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
                        setResolutionInput(preset.value);
                        setResolutionError(false);
                        handleChange("resolution", preset.value);
                      }}
                    >
                      <span>{preset.label}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>

            {/* FPS — dynamic text input */}
            <div className="settings-row">
              <label className="settings-label">FPS</label>
              <div className="settings-input-group">
                <input
                  type="text"
                  className={`settings-text-input settings-text-input--narrow ${fpsError ? "error" : ""}`}
                  value={fpsInput}
                  onChange={(e) => setFpsInput(e.target.value)}
                  onBlur={handleFpsBlur}
                  onKeyDown={handleFpsKeyDown}
                  placeholder="60"
                  spellCheck={false}
                />
                {fpsError && <span className="settings-input-hint">1 - 360</span>}
              </div>
            </div>

            {/* FPS presets — dynamic or static */}
            <div className="settings-row">
              <label className="settings-label">Presets</label>
              <div className="settings-chip-row">
                {(hasDynamic ? dynamicFpsOptions.map((v) => ({ value: v })) : STATIC_FPS_PRESETS).map(
                  (preset) => (
                    <button
                      key={preset.value}
                      className={`settings-chip ${settings.fps === preset.value ? "active" : ""}`}
                      onClick={() => {
                        setFpsInput(String(preset.value));
                        setFpsError(false);
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
          </div>
        </section>

        {/* ── Audio ──────────────────────────────────────── */}
        <section className="settings-section">
          <div className="settings-section-header">
            <Volume2 size={18} />
            <h2>Audio</h2>
          </div>
          <div className="settings-rows">
            <div className="settings-placeholder">Audio configuration coming soon</div>
          </div>
        </section>

        {/* ── Input ──────────────────────────────────────── */}
        <section className="settings-section">
          <div className="settings-section-header">
            <Mouse size={18} />
            <h2>Input</h2>
          </div>
          <div className="settings-rows">
            <div className="settings-row settings-row--column">
              <div className="settings-row-top">
                <label className="settings-label">Mouse Sensitivity</label>
                <span className="settings-value-badge">{(settings.mouseSensitivity ?? 1).toFixed(1)}x</span>
              </div>
              <input
                type="range"
                className="settings-slider"
                min={0.1}
                max={3}
                step={0.1}
                value={settings.mouseSensitivity ?? 1}
                onChange={(e) => handleChange("mouseSensitivity", parseFloat(e.target.value))}
              />
            </div>

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
          </div>
        </section>

        {/* ── Region ────────────────────────────────────── */}
        <section className="settings-section">
          <div className="settings-section-header">
            <Globe size={18} />
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
