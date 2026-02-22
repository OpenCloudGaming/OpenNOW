import { useState, useEffect, useCallback, useRef } from "react";
import type { JSX } from "react";
import { Save, Trash2, RotateCcw, Check, Joystick, Eye, AlertTriangle } from "lucide-react";
import type {
  Settings,
  FlightProfile,
  FlightControlsState,
  FlightSlotConfig,
  FlightAxisTarget,
  FlightSensitivityCurve,
} from "@shared/gfn";
import { makeDeviceKey, defaultFlightSlots } from "@shared/gfn";
import { makeVidPid } from "@shared/flightDefaults";
import { FlightHidService, getFlightHidService, isFlightDevice } from "../flight/FlightHidService";
import { useToast } from "./Toast";

interface FlightControlsPanelProps {
  settings: Settings;
  onSettingChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

const AXIS_TARGETS: { value: FlightAxisTarget; label: string }[] = [
  { value: "leftStickX", label: "Left Stick X (Roll)" },
  { value: "leftStickY", label: "Left Stick Y (Pitch)" },
  { value: "rightStickX", label: "Right Stick X (Yaw)" },
  { value: "rightStickY", label: "Right Stick Y" },
  { value: "leftTrigger", label: "Left Trigger" },
  { value: "rightTrigger", label: "Right Trigger (Throttle)" },
];

const CURVE_OPTIONS: { value: FlightSensitivityCurve; label: string }[] = [
  { value: "linear", label: "Linear" },
  { value: "expo", label: "Exponential" },
];

const BUTTON_LABELS: Record<number, string> = {
  0x1000: "A", 0x2000: "B", 0x4000: "X", 0x8000: "Y",
  0x0100: "LB", 0x0200: "RB", 0x0040: "L3", 0x0080: "R3",
  0x0010: "Start", 0x0020: "Back",
  0x0001: "D-Up", 0x0002: "D-Down", 0x0004: "D-Left", 0x0008: "D-Right",
};

interface DeviceEntry {
  device: HIDDevice;
  vidPid: string;
  deviceKey: string;
  label: string;
  isFlight: boolean;
}

function toEntry(device: HIDDevice): DeviceEntry {
  const vidPid = makeVidPid(device.vendorId, device.productId);
  const name = device.productName || `HID ${vidPid}`;
  const flight = isFlightDevice(device);
  return {
    device,
    vidPid,
    deviceKey: makeDeviceKey(device.vendorId, device.productId, device.productName || ""),
    label: flight ? name : `${name} [other]`,
    isFlight: flight,
  };
}

function getSlots(settings: Settings): FlightSlotConfig[] {
  if (Array.isArray(settings.flightSlots) && settings.flightSlots.length === 4) {
    return settings.flightSlots;
  }
  return defaultFlightSlots();
}

function updateSlot(
  settings: Settings,
  slotIdx: number,
  patch: Partial<FlightSlotConfig>,
  onSettingChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void,
): void {
  const slots = getSlots(settings).map((s, i) => (i === slotIdx ? { ...s, ...patch } : { ...s }));
  onSettingChange("flightSlots", slots);
}

export function FlightControlsPanel({ settings, onSettingChange }: FlightControlsPanelProps): JSX.Element {
  const { showToast } = useToast();
  const [activeTab, setActiveTab] = useState(0);
  const [devices, setDevices] = useState<DeviceEntry[]>([]);
  const [showAllDevices, setShowAllDevices] = useState(false);
  const [profiles, setProfiles] = useState<Map<string, FlightProfile>>(new Map());
  const [editingProfile, setEditingProfile] = useState<FlightProfile | null>(null);
  const [capturingSlots, setCapturingSlots] = useState<Set<number>>(new Set());
  const [liveStates, setLiveStates] = useState<Map<number, FlightControlsState>>(new Map());
  const [inputAlive, setInputAlive] = useState<Map<number, boolean>>(new Map());
  const [pairError, setPairError] = useState<string | null>(null);
  const [savedIndicator, setSavedIndicator] = useState(false);
  const [allProfiles, setAllProfiles] = useState<FlightProfile[]>([]);
  const inputAliveTimers = useRef<Map<number, ReturnType<typeof setTimeout>>>(new Map());
  const stateUnsubRef = useRef<(() => void) | null>(null);
  const gamepadUnsubRef = useRef<(() => void) | null>(null);
  const profileLoadEpoch = useRef(0);
  const webHidSupported = FlightHidService.isSupported();

  const enabled = settings.flightControlsEnabled;
  const slots = getSlots(settings);
  const slotConfig = slots[activeTab]!;

  const refreshDevices = useCallback(async (showAll: boolean): Promise<DeviceEntry[]> => {
    const devs = await getFlightHidService().getDevices(showAll);
    const entries = devs.map(toEntry);
    setDevices(entries);
    return entries;
  }, []);

  const loadAllProfiles = useCallback(async () => {
    setAllProfiles(await window.openNow.flightGetAllProfiles());
  }, []);

  const loadProfileForVidPid = useCallback(async (vidPid: string): Promise<FlightProfile | null> => {
    const epoch = ++profileLoadEpoch.current;
    let p = await window.openNow.flightGetProfile(vidPid);
    if (!p) p = await window.openNow.flightResetProfile(vidPid);
    if (profileLoadEpoch.current !== epoch) return null;
    if (p) {
      setProfiles((prev) => {
        const next = new Map(prev);
        next.set(vidPid, p);
        return next;
      });
    }
    return p;
  }, []);

  useEffect(() => {
    if (!enabled || !webHidSupported) return;
    void (async () => {
      await refreshDevices(showAllDevices);
      await loadAllProfiles();
    })();
  }, [enabled, webHidSupported]);

  useEffect(() => {
    if (!enabled || !webHidSupported) return;
    const vidPid = slotConfig.vidPid;
    if (vidPid) {
      void loadProfileForVidPid(vidPid).then((p) => {
        if (p) setEditingProfile({ ...p });
        else setEditingProfile(null);
      });
    } else {
      setEditingProfile(null);
    }
  }, [activeTab, enabled, webHidSupported, slotConfig.vidPid]);

  useEffect(() => {
    if (!enabled || !webHidSupported) return;
    const service = getFlightHidService();

    if (stateUnsubRef.current) stateUnsubRef.current();
    stateUnsubRef.current = service.onStateUpdate((slot, state) => {
      setLiveStates((prev) => {
        const next = new Map(prev);
        next.set(slot, state);
        return next;
      });
      setInputAlive((prev) => {
        const next = new Map(prev);
        next.set(slot, true);
        return next;
      });
      const prev = inputAliveTimers.current.get(slot);
      if (prev) clearTimeout(prev);
      inputAliveTimers.current.set(slot, setTimeout(() => {
        setInputAlive((p) => {
          const n = new Map(p);
          n.set(slot, false);
          return n;
        });
      }, 500));
    });

    return () => {
      if (stateUnsubRef.current) { stateUnsubRef.current(); stateUnsubRef.current = null; }
    };
  }, [enabled, webHidSupported]);

  useEffect(() => {
    return () => {
      for (const t of inputAliveTimers.current.values()) clearTimeout(t);
    };
  }, []);

  const handlePairDevice = useCallback(async () => {
    setPairError(null);
    const service = getFlightHidService();
    let newDevices: HIDDevice[];
    try {
      newDevices = await service.requestDevice();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setPairError(`Pairing failed: ${msg}`);
      showToast("Device pairing failed", "error");
      return;
    }
    if (newDevices.length === 0) {
      setPairError("No device selected. Make sure your device is connected and try again.");
      return;
    }
    const entries = await refreshDevices(showAllDevices);
    const added = newDevices[0]!;
    const key = makeDeviceKey(added.vendorId, added.productId, added.productName || "");
    const vidPid = makeVidPid(added.vendorId, added.productId);
    const entry = entries.find((e) => e.deviceKey === key);
    if (entry) {
      updateSlot(settings, activeTab, {
        deviceKey: key,
        vidPid,
        deviceName: added.productName || null,
      }, onSettingChange);
    }
    showToast(`Device paired: ${added.productName || "HID device"}`, "success");
    setPairError(null);
  }, [showAllDevices, refreshDevices, showToast, settings, activeTab, onSettingChange]);

  const handleSelectDevice = useCallback((deviceKey: string) => {
    const entry = devices.find((d) => d.deviceKey === deviceKey);
    if (!entry) {
      updateSlot(settings, activeTab, { deviceKey: null, vidPid: null, deviceName: null }, onSettingChange);
      return;
    }
    updateSlot(settings, activeTab, {
      deviceKey: entry.deviceKey,
      vidPid: entry.vidPid,
      deviceName: entry.device.productName || null,
    }, onSettingChange);
  }, [devices, settings, activeTab, onSettingChange]);

  const handleStartCapture = useCallback(async (slotIdx: number) => {
    const cfg = slots[slotIdx]!;
    if (!cfg.deviceKey || !cfg.vidPid) return;
    const entry = devices.find((d) => d.deviceKey === cfg.deviceKey);
    if (!entry) {
      showToast(`Device not found for slot ${slotIdx + 1}`, "error");
      return;
    }
    let profile = profiles.get(cfg.vidPid) ?? null;
    if (!profile) {
      profile = await loadProfileForVidPid(cfg.vidPid);
    }
    if (!profile) {
      showToast("No profile available", "error");
      return;
    }
    const ok = await getFlightHidService().startCapture(slotIdx, entry.device, profile);
    if (!ok) {
      showToast(`Failed to start capture for slot ${slotIdx + 1}`, "error");
      return;
    }
    setCapturingSlots((prev) => new Set(prev).add(slotIdx));
  }, [slots, devices, profiles, loadProfileForVidPid, showToast]);

  const handleStopCapture = useCallback((slotIdx: number) => {
    getFlightHidService().stopCapture(slotIdx);
    setCapturingSlots((prev) => {
      const next = new Set(prev);
      next.delete(slotIdx);
      return next;
    });
    setLiveStates((prev) => {
      const next = new Map(prev);
      next.delete(slotIdx);
      return next;
    });
    setInputAlive((prev) => {
      const next = new Map(prev);
      next.delete(slotIdx);
      return next;
    });
  }, []);

  const handleToggleSlotEnabled = useCallback((slotIdx: number, value: boolean) => {
    updateSlot(settings, slotIdx, { enabled: value }, onSettingChange);
    if (!value) {
      handleStopCapture(slotIdx);
    }
  }, [settings, onSettingChange, handleStopCapture]);

  const handleAxisMappingChange = useCallback(
    (idx: number, field: string, value: string | number | boolean) => {
      setEditingProfile((prev) => {
        if (!prev) return prev;
        return { ...prev, axisMappings: prev.axisMappings.map((m, i) => (i === idx ? { ...m, [field]: value } : m)) };
      });
    }, [],
  );

  const handleAddButtonMapping = useCallback(() => {
    setEditingProfile((prev) => {
      if (!prev) return prev;
      return { ...prev, buttonMappings: [...prev.buttonMappings, { sourceIndex: prev.buttonMappings.length, targetButton: 0x1000 }] };
    });
  }, []);

  const handleRemoveButtonMapping = useCallback((idx: number) => {
    setEditingProfile((prev) => {
      if (!prev) return prev;
      return { ...prev, buttonMappings: prev.buttonMappings.filter((_, i) => i !== idx) };
    });
  }, []);

  const handleButtonMappingChange = useCallback(
    (idx: number, field: string, value: number) => {
      setEditingProfile((prev) => {
        if (!prev) return prev;
        return { ...prev, buttonMappings: prev.buttonMappings.map((m, i) => (i === idx ? { ...m, [field]: value } : m)) };
      });
    }, [],
  );

  const handleSaveProfile = useCallback(async () => {
    if (!editingProfile) return;
    await window.openNow.flightSetProfile(editingProfile);
    setProfiles((prev) => {
      const next = new Map(prev);
      next.set(editingProfile.vidPid, editingProfile);
      return next;
    });
    await loadAllProfiles();
    setSavedIndicator(true);
    setTimeout(() => setSavedIndicator(false), 1500);
    showToast("Flight profile saved", "success");
  }, [editingProfile, loadAllProfiles, showToast]);

  const handleResetProfile = useCallback(async () => {
    if (!editingProfile) return;
    const p = await window.openNow.flightResetProfile(editingProfile.vidPid);
    if (p) {
      setEditingProfile({ ...p });
      setProfiles((prev) => { const next = new Map(prev); next.set(p.vidPid, p); return next; });
    }
    await loadAllProfiles();
    showToast("Profile reset to defaults", "info");
  }, [editingProfile, loadAllProfiles, showToast]);

  const handleDeleteProfile = useCallback(
    async (vidPid: string, gameId?: string) => {
      await window.openNow.flightDeleteProfile(vidPid, gameId);
      await loadAllProfiles();
      showToast("Profile deleted", "error");
    }, [loadAllProfiles, showToast],
  );

  const isSlotCapturing = capturingSlots.has(activeTab);
  const slotLiveState = liveStates.get(activeTab) ?? null;
  const slotInputAlive = inputAlive.get(activeTab) ?? false;
  const selectedDeviceEntry = slotConfig.deviceKey ? devices.find((d) => d.deviceKey === slotConfig.deviceKey) : undefined;
  const mappingKey = `${slotConfig.vidPid || "none"}:${activeTab}`;

  return (
    <>
      {/* Global enable */}
      <div className="settings-row">
        <label className="settings-label">Enable Flight Controls</label>
        <label className="settings-toggle">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(e) => onSettingChange("flightControlsEnabled", e.target.checked)}
          />
          <span className="settings-toggle-track" />
        </label>
      </div>

      {enabled && !webHidSupported && (
        <div className="settings-row">
          <span className="settings-subtle-hint" style={{ color: "var(--error)" }}>
            WebHID is not available in this browser / Electron version.
          </span>
        </div>
      )}

      {enabled && webHidSupported && (
        <>
          {/* Slot tabs */}
          <div style={{
            display: "flex", gap: 0, borderRadius: "var(--r-sm)",
            border: "1px solid var(--panel-border)", overflow: "hidden", marginBottom: 2,
          }}>
            {[0, 1, 2, 3].map((s) => {
              const cfg = slots[s]!;
              const active = s === activeTab;
              const isCapt = capturingSlots.has(s);
              return (
                <button
                  key={s}
                  type="button"
                  onClick={() => setActiveTab(s)}
                  style={{
                    flex: 1, padding: "8px 4px",
                    background: active ? "var(--accent-surface-strong)" : "transparent",
                    border: "none",
                    borderRight: s < 3 ? "1px solid var(--panel-border)" : "none",
                    color: active ? "var(--accent)" : "var(--ink-soft)",
                    fontWeight: active ? 700 : 500,
                    fontSize: "0.82rem",
                    fontFamily: "inherit",
                    cursor: "pointer",
                    transition: "all 0.15s ease",
                    position: "relative",
                  }}
                >
                  <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 2 }}>
                    <span>Slot {s + 1}</span>
                    <span style={{
                      fontSize: "0.65rem",
                      color: isCapt ? "var(--accent)" : cfg.enabled && cfg.deviceKey ? "var(--ink-muted)" : "var(--ink-muted)",
                      opacity: cfg.enabled && cfg.deviceKey ? 1 : 0.5,
                    }}>
                      {isCapt ? "Active" : cfg.enabled && cfg.deviceName ? cfg.deviceName.slice(0, 16) : cfg.enabled ? "No device" : "Off"}
                    </span>
                  </div>
                  {isCapt && (
                    <span style={{
                      position: "absolute", top: 4, right: 6,
                      width: 6, height: 6, borderRadius: "50%",
                      background: "var(--accent)",
                      boxShadow: "0 0 6px var(--accent-glow)",
                    }} />
                  )}
                </button>
              );
            })}
          </div>

          {/* Slot status summary */}
          <div className="settings-row">
            <span className="settings-subtle-hint">
              {(() => {
                const activeSlots = [0, 1, 2, 3].filter((s) => capturingSlots.has(s));
                if (activeSlots.length === 0) return "No slots are currently capturing input.";
                return `Active: ${activeSlots.map((s) => `Slot ${s + 1}`).join(", ")}`;
              })()}
            </span>
          </div>

          {/* Per-slot enable toggle */}
          <div className="settings-row">
            <label className="settings-label">Enable Slot {activeTab + 1}</label>
            <label className="settings-toggle">
              <input
                type="checkbox"
                checked={slotConfig.enabled}
                onChange={(e) => handleToggleSlotEnabled(activeTab, e.target.checked)}
              />
              <span className="settings-toggle-track" />
            </label>
          </div>

          {slotConfig.enabled && (
            <>
              {/* Device selection */}
              <div className="settings-row" style={{ flexDirection: "column", alignItems: "stretch", gap: 10 }}>
                <label className="settings-label" style={{ margin: 0 }}>Device for Slot {activeTab + 1}</label>

                {devices.length === 0 ? (
                  <>
                    <button
                      type="button"
                      className="settings-save-btn"
                      style={{ alignSelf: "flex-start", padding: "9px 18px", fontSize: "0.88rem" }}
                      onClick={() => void handlePairDevice()}
                    >
                      <Joystick size={15} />
                      Pair flight device…
                    </button>
                    <span className="settings-subtle-hint">
                      No paired devices. Connect your HOTAS / joystick and click above to pair it.
                    </span>
                  </>
                ) : (
                  <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                    <select
                      className="settings-text-input"
                      style={{ flex: 1 }}
                      value={slotConfig.deviceKey ?? ""}
                      onChange={(e) => handleSelectDevice(e.target.value)}
                    >
                      <option value="">None (unbound)</option>
                      {devices.map((d, i) => (
                        <option key={`${d.deviceKey}-${i}`} value={d.deviceKey}>{d.label}</option>
                      ))}
                    </select>
                    <button
                      type="button"
                      className="settings-shortcut-reset-btn"
                      style={{ whiteSpace: "nowrap" }}
                      onClick={() => void handlePairDevice()}
                      title="Pair another HID device"
                    >
                      Pair new…
                    </button>
                  </div>
                )}

                {pairError && (
                  <div style={{
                    display: "flex", alignItems: "center", gap: 6,
                    padding: "6px 10px", borderRadius: 6,
                    background: "var(--error-bg)", border: "1px solid var(--error-border)",
                  }}>
                    <AlertTriangle size={13} style={{ color: "var(--error)", flexShrink: 0 }} />
                    <span style={{ fontSize: "0.78rem", color: "var(--error)" }}>{pairError}</span>
                  </div>
                )}

                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <label className="settings-toggle" style={{ transform: "scale(0.8)", transformOrigin: "left center" }}>
                    <input
                      type="checkbox"
                      checked={showAllDevices}
                      onChange={(e) => {
                        setShowAllDevices(e.target.checked);
                        void refreshDevices(e.target.checked);
                      }}
                    />
                    <span className="settings-toggle-track" />
                  </label>
                  <span style={{ fontSize: "0.72rem", color: "var(--ink-muted)" }}>
                    <Eye size={11} style={{ verticalAlign: "-1px", marginRight: 4 }} />
                    Show all HID devices
                  </span>
                </div>
              </div>

              {/* Capture controls */}
              {selectedDeviceEntry && (
                <div className="settings-row" style={{ flexDirection: "column", alignItems: "stretch", gap: 8 }}>
                  <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
                    <label className="settings-label" style={{ margin: 0 }}>Capture</label>
                    <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                      {!isSlotCapturing ? (
                        <button
                          type="button"
                          className="settings-shortcut-reset-btn"
                          style={{
                            color: "var(--accent)", borderColor: "color-mix(in srgb, var(--accent) 35%, transparent)",
                            padding: "5px 14px", fontWeight: 600,
                          }}
                          onClick={() => void handleStartCapture(activeTab)}
                        >
                          Start
                        </button>
                      ) : (
                        <button
                          type="button"
                          className="settings-shortcut-reset-btn"
                          style={{
                            color: "var(--error)", borderColor: "var(--error-border)",
                            padding: "5px 14px", fontWeight: 600,
                          }}
                          onClick={() => handleStopCapture(activeTab)}
                        >
                          Stop
                        </button>
                      )}
                    </div>
                  </div>

                  <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: "0.78rem" }}>
                    <span style={{
                      width: 7, height: 7, borderRadius: "50%", flexShrink: 0,
                      background: isSlotCapturing ? (slotInputAlive ? "var(--accent)" : "var(--warning)") : "var(--ink-muted)",
                      boxShadow: isSlotCapturing && slotInputAlive ? "0 0 6px var(--accent-glow)" : "none",
                      transition: "all 0.2s ease",
                    }} />
                    <span style={{ color: isSlotCapturing ? "var(--ink-soft)" : "var(--ink-muted)" }}>
                      {!isSlotCapturing && "Not capturing"}
                      {isSlotCapturing && slotInputAlive && "Receiving input…"}
                      {isSlotCapturing && !slotInputAlive && "Capturing (waiting for input)"}
                    </span>
                  </div>

                  {isSlotCapturing && (
                    <span style={{ fontSize: "0.7rem", color: "var(--ink-muted)", fontFamily: "var(--font-mono, monospace)" }}>
                      Injecting flight input into slot {activeTab + 1}
                    </span>
                  )}
                </div>
              )}

              {/* Live Tester */}
              {isSlotCapturing && slotLiveState && (
                <div className="flight-tester">
                  <h3>
                    <Joystick size={14} />
                    Live Input — Slot {activeTab + 1}
                  </h3>

                  {slotLiveState.axes.length > 0 && (
                    <div className="flight-axes-grid">
                      {slotLiveState.axes.map((v, i) => (
                        <div key={i} className="flight-axis-bar">
                          <span className="flight-axis-label">Axis {i}</span>
                          <div className="flight-axis-track">
                            <div className="flight-axis-fill" style={{ width: `${(v * 100).toFixed(1)}%` }} />
                          </div>
                          <span className="flight-axis-value">{v.toFixed(2)}</span>
                        </div>
                      ))}
                    </div>
                  )}

                  {slotLiveState.buttons.length > 0 && (
                    <div className="flight-buttons-grid">
                      {slotLiveState.buttons.map((pressed, i) => (
                        <div key={i} className={`flight-button-indicator${pressed ? " pressed" : ""}`}>
                          {i}
                        </div>
                      ))}
                    </div>
                  )}

                  {slotLiveState.hatSwitch >= 0 && (
                    <div className="flight-hat-indicator">Hat: {slotLiveState.hatSwitch}</div>
                  )}

                  <details className="flight-raw-bytes">
                    <summary>Raw bytes ({slotLiveState.rawBytes.length})</summary>
                    <div className="flight-raw-bytes-data">
                      {slotLiveState.rawBytes.map((b, i) => (
                        <span key={i}>{b.toString(16).padStart(2, "0").toUpperCase()}</span>
                      ))}
                    </div>
                  </details>
                </div>
              )}

              {/* Mapping editor */}
              {editingProfile && (
                <>
                  <div className="flight-mapping-section" key={mappingKey}>
                    <div className="flight-mapping-header">
                      <h3>Axis Mappings</h3>
                      <div className="flight-mapping-actions">
                        <button
                          type="button"
                          className="settings-shortcut-reset-btn"
                          style={{ color: "var(--accent)", borderColor: "color-mix(in srgb, var(--accent) 35%, transparent)" }}
                          onClick={() => void handleSaveProfile()}
                          title="Save profile"
                        >
                          {savedIndicator ? <Check size={13} /> : <Save size={13} />}
                          {savedIndicator ? "Saved" : "Save"}
                        </button>
                        <button
                          type="button"
                          className="settings-shortcut-reset-btn"
                          onClick={() => void handleResetProfile()}
                          title="Reset to defaults"
                        >
                          <RotateCcw size={13} />
                          Reset
                        </button>
                      </div>
                    </div>

                    <div className="flight-mappings-list">
                      {editingProfile.axisMappings.map((mapping, idx) => (
                        <div key={idx} className="flight-mapping-row">
                          <span className="flight-mapping-source">Axis {mapping.sourceIndex}</span>
                          <div className="flight-mapping-fields">
                            <div className="flight-mapping-field">
                              <label>Target</label>
                              <select
                                className="settings-text-input"
                                value={mapping.target}
                                onChange={(e) => handleAxisMappingChange(idx, "target", e.target.value)}
                              >
                                {AXIS_TARGETS.map((t) => (
                                  <option key={t.value} value={t.value}>{t.label}</option>
                                ))}
                              </select>
                            </div>
                            <div className="flight-mapping-field">
                              <label>Deadzone</label>
                              <input
                                type="range" min={0} max={0.5} step={0.01}
                                value={mapping.deadzone}
                                onChange={(e) => handleAxisMappingChange(idx, "deadzone", parseFloat(e.target.value))}
                              />
                              <span className="flight-mapping-value">{mapping.deadzone.toFixed(2)}</span>
                            </div>
                            <div className="flight-mapping-field">
                              <label>Sensitivity</label>
                              <input
                                type="range" min={0.1} max={3.0} step={0.1}
                                value={mapping.sensitivity}
                                onChange={(e) => handleAxisMappingChange(idx, "sensitivity", parseFloat(e.target.value))}
                              />
                              <span className="flight-mapping-value">{mapping.sensitivity.toFixed(1)}</span>
                            </div>
                            <div className="flight-mapping-field">
                              <label>Curve</label>
                              <select
                                className="settings-text-input"
                                value={mapping.curve}
                                onChange={(e) => handleAxisMappingChange(idx, "curve", e.target.value)}
                              >
                                {CURVE_OPTIONS.map((c) => (
                                  <option key={c.value} value={c.value}>{c.label}</option>
                                ))}
                              </select>
                            </div>
                            <div className="flight-mapping-field flight-mapping-field--checkbox">
                              <label>Inverted</label>
                              <input
                                type="checkbox"
                                checked={mapping.inverted}
                                onChange={(e) => handleAxisMappingChange(idx, "inverted", e.target.checked)}
                              />
                            </div>
                          </div>
                        </div>
                      ))}
                    </div>

                    <div className="flight-mapping-header" style={{ marginTop: 12 }}>
                      <h3>Button Mappings</h3>
                      <button
                        type="button"
                        className="settings-shortcut-reset-btn"
                        style={{ color: "var(--accent)", borderColor: "color-mix(in srgb, var(--accent) 35%, transparent)" }}
                        onClick={handleAddButtonMapping}
                      >
                        + Add
                      </button>
                    </div>

                    <div className="flight-mappings-list">
                      {editingProfile.buttonMappings.map((mapping, idx) => (
                        <div key={idx} className="flight-mapping-row">
                          <span className="flight-mapping-source">Btn {mapping.sourceIndex}</span>
                          <div className="flight-mapping-fields">
                            <div className="flight-mapping-field">
                              <label>Source #</label>
                              <input
                                type="number" className="settings-text-input" style={{ width: 56 }} min={0}
                                value={mapping.sourceIndex}
                                onChange={(e) => handleButtonMappingChange(idx, "sourceIndex", parseInt(e.target.value) || 0)}
                              />
                            </div>
                            <div className="flight-mapping-field">
                              <label>Target</label>
                              <select
                                className="settings-text-input"
                                value={mapping.targetButton}
                                onChange={(e) => handleButtonMappingChange(idx, "targetButton", parseInt(e.target.value))}
                              >
                                {Object.entries(BUTTON_LABELS).map(([val, lbl]) => (
                                  <option key={val} value={val}>{lbl}</option>
                                ))}
                              </select>
                            </div>
                          </div>
                          <button
                            type="button"
                            className="settings-shortcut-reset-btn"
                            style={{ color: "var(--error)", borderColor: "var(--error-border)", padding: "3px 8px" }}
                            onClick={() => handleRemoveButtonMapping(idx)}
                          >
                            <Trash2 size={12} />
                          </button>
                        </div>
                      ))}
                    </div>
                  </div>

                  {allProfiles.length > 0 && (
                    <div className="flight-profiles-section">
                      <h3>Saved Profiles ({allProfiles.length})</h3>
                      <div className="flight-profiles-list">
                        {allProfiles.map((p, i) => (
                          <div key={i} className="flight-profile-row">
                            <div className="flight-profile-info">
                              <span className="flight-profile-name">{p.name || p.vidPid}</span>
                              <span className="flight-profile-meta">
                                {p.vidPid}
                                {p.gameId ? ` · Game: ${p.gameId}` : " · Global"}
                              </span>
                            </div>
                            <button
                              type="button"
                              className="settings-shortcut-reset-btn"
                              style={{ color: "#ef4444", borderColor: "rgba(239,68,68,0.25)", padding: "4px 8px" }}
                              onClick={() => void handleDeleteProfile(p.vidPid, p.gameId)}
                              title="Delete profile"
                            >
                              <Trash2 size={12} />
                            </button>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </>
              )}
            </>
          )}
        </>
      )}
    </>
  );
}
