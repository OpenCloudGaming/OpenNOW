import { useState, useEffect, useCallback, useRef } from "react";
import type { JSX } from "react";
import { Joystick, RefreshCw, Save, Trash2, RotateCcw, Check, Plus } from "lucide-react";
import type {
  Settings,
  FlightProfile,
  FlightControlsState,
  FlightAxisTarget,
  FlightSensitivityCurve,
} from "@shared/gfn";
import { makeVidPid } from "@shared/flightDefaults";
import { FlightHidService, getFlightHidService } from "../flight/FlightHidService";

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
  { value: "rightTrigger", label: "Right Trigger" },
];

const CURVE_OPTIONS: { value: FlightSensitivityCurve; label: string }[] = [
  { value: "linear", label: "Linear" },
  { value: "expo", label: "Exponential" },
];

interface DeviceEntry {
  device: HIDDevice;
  vidPid: string;
  label: string;
}

export function FlightControlsPanel({ settings, onSettingChange }: FlightControlsPanelProps): JSX.Element {
  const [devices, setDevices] = useState<DeviceEntry[]>([]);
  const [selectedIdx, setSelectedIdx] = useState(-1);
  const [isCapturing, setIsCapturing] = useState(false);
  const [flightState, setFlightState] = useState<FlightControlsState | null>(null);
  const [profile, setProfile] = useState<FlightProfile | null>(null);
  const [allProfiles, setAllProfiles] = useState<FlightProfile[]>([]);
  const [savedIndicator, setSavedIndicator] = useState(false);
  const stateUnsubRef = useRef<(() => void) | null>(null);
  const webHidSupported = FlightHidService.isSupported();

  const enabled = settings.flightControlsEnabled;
  const slot = settings.flightControlsSlot;

  const loadDevices = useCallback(async () => {
    const service = getFlightHidService();
    const devs = await service.getDevices();
    const entries: DeviceEntry[] = devs.map((d) => ({
      device: d,
      vidPid: makeVidPid(d.vendorId, d.productId),
      label: d.productName || makeVidPid(d.vendorId, d.productId),
    }));
    setDevices(entries);
  }, []);

  const loadAllProfiles = useCallback(async () => {
    const api = window.openNow;
    const profiles = await api.flightGetAllProfiles();
    setAllProfiles(profiles);
  }, []);

  const handleRefresh = useCallback(async () => {
    await loadDevices();
    await loadAllProfiles();
  }, [loadDevices, loadAllProfiles]);

  useEffect(() => {
    if (enabled && webHidSupported) {
      void handleRefresh();
    }
  }, [enabled, webHidSupported, handleRefresh]);

  useEffect(() => {
    const service = getFlightHidService();
    service.controllerSlot = slot;
  }, [slot]);

  useEffect(() => {
    return () => {
      if (stateUnsubRef.current) {
        stateUnsubRef.current();
        stateUnsubRef.current = null;
      }
    };
  }, []);

  const handleRequestDevice = useCallback(async () => {
    const service = getFlightHidService();
    const device = await service.requestDevice();
    if (device) {
      await loadDevices();
    }
  }, [loadDevices]);

  const handleStartCapture = useCallback(async () => {
    if (selectedIdx < 0 || selectedIdx >= devices.length) return;
    const entry = devices[selectedIdx]!;

    const api = window.openNow;
    let p = await api.flightGetProfile(entry.vidPid);
    if (!p) {
      p = await api.flightResetProfile(entry.vidPid);
    }
    setProfile(p);

    const service = getFlightHidService();
    service.controllerSlot = slot;

    if (stateUnsubRef.current) {
      stateUnsubRef.current();
    }
    stateUnsubRef.current = service.onStateUpdate((state) => {
      setFlightState(state);
    });

    const ok = p ? await service.startCapture(entry.device, p) : false;
    setIsCapturing(ok);
  }, [selectedIdx, devices, slot]);

  const handleStopCapture = useCallback(() => {
    const service = getFlightHidService();
    service.stopCapture();
    setIsCapturing(false);
    setFlightState(null);
    if (stateUnsubRef.current) {
      stateUnsubRef.current();
      stateUnsubRef.current = null;
    }
  }, []);

  const handleAxisMappingChange = useCallback(
    (index: number, field: string, value: unknown) => {
      if (!profile) return;
      const updated = { ...profile };
      updated.axisMappings = updated.axisMappings.map((m, i) => {
        if (i !== index) return m;
        return { ...m, [field]: value };
      });
      setProfile(updated);
    },
    [profile],
  );

  const handleButtonMappingChange = useCallback(
    (index: number, field: string, value: unknown) => {
      if (!profile) return;
      const updated = { ...profile };
      updated.buttonMappings = updated.buttonMappings.map((m, i) => {
        if (i !== index) return m;
        return { ...m, [field]: value };
      });
      setProfile(updated);
    },
    [profile],
  );

  const handleSaveProfile = useCallback(async () => {
    if (!profile) return;
    const api = window.openNow;
    await api.flightSetProfile(profile);
    setSavedIndicator(true);
    setTimeout(() => setSavedIndicator(false), 1500);
    await loadAllProfiles();

    if (isCapturing) {
      const service = getFlightHidService();
      const activeDevice = service.getActiveDevice();
      if (activeDevice) {
        await service.startCapture(activeDevice, profile);
      }
    }
  }, [profile, isCapturing, loadAllProfiles]);

  const handleResetProfile = useCallback(async () => {
    if (selectedIdx < 0 || selectedIdx >= devices.length) return;
    const entry = devices[selectedIdx]!;
    const api = window.openNow;
    const p = await api.flightResetProfile(entry.vidPid);
    if (p) {
      setProfile(p);
      if (isCapturing) {
        const service = getFlightHidService();
        const activeDevice = service.getActiveDevice();
        if (activeDevice) {
          await service.startCapture(activeDevice, p);
        }
      }
    }
    await loadAllProfiles();
  }, [selectedIdx, devices, isCapturing, loadAllProfiles]);

  const handleDeleteProfile = useCallback(
    async (vidPid: string, gameId?: string) => {
      const api = window.openNow;
      await api.flightDeleteProfile(vidPid, gameId);
      await loadAllProfiles();
    },
    [loadAllProfiles],
  );

  return (
    <div className="flight-panel">
      {/* Enable toggle */}
      <div className="flight-row">
        <label className="flight-label">Enable Flight Controls</label>
        <label className="flight-toggle">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(e) => onSettingChange("flightControlsEnabled", e.target.checked)}
          />
          <span className="flight-toggle-slider" />
        </label>
      </div>

      {enabled && !webHidSupported && (
        <div className="flight-info" style={{ color: "var(--error)" }}>
          WebHID is not available in this browser/Electron version.
        </div>
      )}

      {enabled && webHidSupported && (
        <>
          {/* Controller slot */}
          <div className="flight-row">
            <label className="flight-label">Controller Slot</label>
            <select
              className="flight-select"
              value={slot}
              onChange={(e) => onSettingChange("flightControlsSlot", Number(e.target.value))}
            >
              {[0, 1, 2, 3].map((s) => (
                <option key={s} value={s}>Slot {s}</option>
              ))}
            </select>
          </div>

          {/* Device management */}
          <div className="flight-row flight-row--actions">
            <button type="button" className="flight-btn" onClick={() => void handleRefresh()} title="Refresh paired devices">
              <RefreshCw size={14} /> Refresh
            </button>
            <button type="button" className="flight-btn" onClick={() => void handleRequestDevice()} title="Pair a new HID device">
              <Plus size={14} /> Add Device
            </button>
          </div>

          {/* Device dropdown */}
          {devices.length > 0 && (
            <div className="flight-row">
              <label className="flight-label">Device</label>
              <select
                className="flight-select"
                value={selectedIdx}
                onChange={(e) => setSelectedIdx(Number(e.target.value))}
                disabled={isCapturing}
              >
                <option value={-1}>Select device...</option>
                {devices.map((entry, i) => (
                  <option key={`${entry.vidPid}-${i}`} value={i}>
                    {entry.label} ({entry.vidPid})
                  </option>
                ))}
              </select>
            </div>
          )}

          {devices.length === 0 && (
            <div className="flight-info">
              No paired devices. Click <strong>Add Device</strong> to pair a joystick or HOTAS.
            </div>
          )}

          {/* Start / Stop capture */}
          <div className="flight-row flight-row--actions">
            {!isCapturing ? (
              <button
                type="button"
                className="flight-btn flight-btn--primary"
                onClick={() => void handleStartCapture()}
                disabled={selectedIdx < 0}
              >
                <Joystick size={14} /> Start Capture
              </button>
            ) : (
              <button
                type="button"
                className="flight-btn flight-btn--danger"
                onClick={handleStopCapture}
              >
                Stop Capture
              </button>
            )}
          </div>

          {/* Live tester */}
          {isCapturing && flightState && (
            <div className="flight-tester">
              <div className="flight-tester-header">
                <Joystick size={14} />
                <span>Live Input — {flightState.deviceName}</span>
              </div>
              {flightState.axes.length > 0 && (
                <div className="flight-tester-section">
                  <div className="flight-tester-label">Axes</div>
                  <div className="flight-tester-bars">
                    {flightState.axes.map((val, i) => (
                      <div key={i} className="flight-bar-row">
                        <span className="flight-bar-label">A{i}</span>
                        <div className="flight-bar-track">
                          <div className="flight-bar-fill" style={{ width: `${(val * 100).toFixed(1)}%` }} />
                        </div>
                        <span className="flight-bar-val">{val.toFixed(3)}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
              {flightState.buttons.length > 0 && (
                <div className="flight-tester-section">
                  <div className="flight-tester-label">Buttons</div>
                  <div className="flight-tester-buttons">
                    {flightState.buttons.map((pressed, i) => (
                      <span key={i} className={`flight-btn-dot${pressed ? " active" : ""}`} title={`B${i}`}>
                        {i}
                      </span>
                    ))}
                  </div>
                </div>
              )}
              {flightState.hatSwitch >= 0 && (
                <div className="flight-tester-section">
                  <div className="flight-tester-label">Hat Switch: {flightState.hatSwitch}</div>
                </div>
              )}
              {flightState.rawBytes.length > 0 && (
                <div className="flight-tester-section">
                  <div className="flight-tester-label">Raw Bytes ({flightState.rawBytes.length})</div>
                  <div className="flight-raw-bytes">
                    {flightState.rawBytes.map((b, i) => (
                      <span key={i} className="flight-raw-byte">{b.toString(16).padStart(2, "0").toUpperCase()}</span>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}

          {/* Axis Mappings */}
          {profile && (
            <div className="flight-mappings">
              <div className="flight-mappings-header">Axis Mappings</div>
              <table className="flight-mappings-table">
                <thead>
                  <tr>
                    <th>Source</th>
                    <th>Target</th>
                    <th>Inv</th>
                    <th>Deadzone</th>
                    <th>Sens</th>
                    <th>Curve</th>
                  </tr>
                </thead>
                <tbody>
                  {profile.axisMappings.map((m, i) => (
                    <tr key={i}>
                      <td>Axis {m.sourceIndex}</td>
                      <td>
                        <select
                          className="flight-select flight-select--small"
                          value={m.target}
                          onChange={(e) => handleAxisMappingChange(i, "target", e.target.value)}
                        >
                          {AXIS_TARGETS.map((t) => (
                            <option key={t.value} value={t.value}>{t.label}</option>
                          ))}
                        </select>
                      </td>
                      <td>
                        <input
                          type="checkbox"
                          checked={m.inverted}
                          onChange={(e) => handleAxisMappingChange(i, "inverted", e.target.checked)}
                        />
                      </td>
                      <td>
                        <input
                          type="number"
                          className="flight-input flight-input--small"
                          value={m.deadzone}
                          min={0}
                          max={0.5}
                          step={0.01}
                          onChange={(e) => handleAxisMappingChange(i, "deadzone", parseFloat(e.target.value) || 0)}
                        />
                      </td>
                      <td>
                        <input
                          type="number"
                          className="flight-input flight-input--small"
                          value={m.sensitivity}
                          min={0.1}
                          max={3}
                          step={0.1}
                          onChange={(e) => handleAxisMappingChange(i, "sensitivity", parseFloat(e.target.value) || 1)}
                        />
                      </td>
                      <td>
                        <select
                          className="flight-select flight-select--small"
                          value={m.curve}
                          onChange={(e) => handleAxisMappingChange(i, "curve", e.target.value)}
                        >
                          {CURVE_OPTIONS.map((c) => (
                            <option key={c.value} value={c.value}>{c.label}</option>
                          ))}
                        </select>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>

              <div className="flight-mappings-header" style={{ marginTop: "12px" }}>Button Mappings</div>
              <table className="flight-mappings-table">
                <thead>
                  <tr>
                    <th>Source Btn</th>
                    <th>Target (XInput)</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {profile.buttonMappings.map((m, i) => (
                    <tr key={i}>
                      <td>
                        <input
                          type="number"
                          className="flight-input flight-input--small"
                          value={m.sourceIndex}
                          min={0}
                          onChange={(e) => handleButtonMappingChange(i, "sourceIndex", parseInt(e.target.value, 10) || 0)}
                        />
                      </td>
                      <td>
                        <input
                          type="text"
                          className="flight-input flight-input--small"
                          value={`0x${m.targetButton.toString(16).toUpperCase().padStart(4, "0")}`}
                          onChange={(e) => {
                            const val = parseInt(e.target.value.replace(/^0x/i, ""), 16);
                            if (!Number.isNaN(val)) handleButtonMappingChange(i, "targetButton", val);
                          }}
                        />
                      </td>
                      <td>
                        <button
                          type="button"
                          className="flight-btn flight-btn--small flight-btn--danger"
                          onClick={() => {
                            if (!profile) return;
                            const updated = { ...profile };
                            updated.buttonMappings = updated.buttonMappings.filter((_, idx) => idx !== i);
                            setProfile(updated);
                          }}
                          title="Remove"
                        >
                          <Trash2 size={12} />
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <button
                type="button"
                className="flight-btn flight-btn--small"
                style={{ marginTop: "6px" }}
                onClick={() => {
                  if (!profile) return;
                  const updated = { ...profile };
                  updated.buttonMappings = [
                    ...updated.buttonMappings,
                    { sourceIndex: updated.buttonMappings.length, targetButton: 0x1000 },
                  ];
                  setProfile(updated);
                }}
              >
                + Add Button
              </button>

              {/* Save / Reset */}
              <div className="flight-row flight-row--actions" style={{ marginTop: "12px" }}>
                <button type="button" className="flight-btn flight-btn--primary" onClick={() => void handleSaveProfile()}>
                  {savedIndicator ? <Check size={14} /> : <Save size={14} />}
                  {savedIndicator ? "Saved" : "Save Profile"}
                </button>
                <button type="button" className="flight-btn" onClick={() => void handleResetProfile()}>
                  <RotateCcw size={14} /> Reset to Default
                </button>
              </div>
            </div>
          )}

          {/* All profiles */}
          {allProfiles.length > 0 && (
            <div className="flight-profiles">
              <div className="flight-mappings-header">Saved Profiles</div>
              <div className="flight-profiles-list">
                {allProfiles.map((p, i) => (
                  <div key={`${p.vidPid}-${p.gameId ?? "global"}-${i}`} className="flight-profile-row">
                    <div className="flight-profile-info">
                      <span className="flight-profile-name">{p.name}</span>
                      <span className="flight-profile-meta">
                        {p.vidPid}
                        {p.gameId ? ` · Game: ${p.gameId}` : " · Global"}
                      </span>
                    </div>
                    <button
                      type="button"
                      className="flight-btn flight-btn--small flight-btn--danger"
                      onClick={() => void handleDeleteProfile(p.vidPid, p.gameId)}
                      title="Delete profile"
                    >
                      <Trash2 size={14} />
                    </button>
                  </div>
                ))}
              </div>
            </div>
          )}
        </>
      )}
    </div>
  );
}
