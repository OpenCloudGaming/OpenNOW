import type {
  FlightControlsState,
  FlightGamepadState,
  FlightProfile,
  FlightHidReportLayout,
} from "@shared/gfn";
import { getDeviceConfig, makeVidPid } from "@shared/flightDefaults";
import {
  GAMEPAD_DPAD_UP,
  GAMEPAD_DPAD_DOWN,
  GAMEPAD_DPAD_LEFT,
  GAMEPAD_DPAD_RIGHT,
} from "@shared/flightConstants";

const GENERIC_INPUT_PAGE = 0x01;
const USAGE_JOYSTICK = 0x04;
const USAGE_GAMEPAD = 0x05;
const USAGE_MULTIAXIS = 0x08;
const USAGE_KEYBOARD = 0x06;
const USAGE_MOUSE = 0x02;

const FLIGHT_USAGES = new Set([USAGE_JOYSTICK, USAGE_GAMEPAD, USAGE_MULTIAXIS]);
const EXCLUDED_USAGES = new Set([USAGE_KEYBOARD, USAGE_MOUSE]);

interface ParsedReport {
  axes: number[];
  buttons: boolean[];
  hatSwitch: number;
}

function logDevice(device: HIDDevice, tag: string): void {
  const collections = device.collections.map((c) => ({
    usagePage: `0x${c.usagePage.toString(16).padStart(2, "0")}`,
    usage: `0x${c.usage.toString(16).padStart(2, "0")}`,
  }));
  console.log(
    `[Flight:${tag}] ${device.productName || "(no name)"}  VID=0x${device.vendorId.toString(16).padStart(4, "0")} PID=0x${device.productId.toString(16).padStart(4, "0")}  collections=`,
    collections,
  );
}

export function isFlightDevice(device: HIDDevice): boolean {
  return device.collections.some(
    (c) => c.usagePage === GENERIC_INPUT_PAGE && FLIGHT_USAGES.has(c.usage),
  );
}

function isExcludedDevice(device: HIDDevice): boolean {
  if (device.collections.length === 0) return false;
  return device.collections.every(
    (c) => c.usagePage === GENERIC_INPUT_PAGE && EXCLUDED_USAGES.has(c.usage),
  );
}

export function classifyDevice(device: HIDDevice, showAll: boolean): boolean {
  if (showAll) return true;
  if (isExcludedDevice(device)) return false;
  if (isFlightDevice(device)) return true;
  if (device.collections.length === 0) return true;
  return false;
}

export class FlightHidService {
  private device: HIDDevice | null = null;
  private profile: FlightProfile | null = null;
  private reportLayout: FlightHidReportLayout | null = null;
  private _controllerSlot = 3;
  private _capturing = false;
  private lastGamepadState: FlightGamepadState | null = null;
  private stateListeners = new Set<(state: FlightControlsState) => void>();
  private gamepadListeners = new Set<(state: FlightGamepadState) => void>();
  private boundOnInputReport: ((event: HIDInputReportEvent) => void) | null = null;

  static isSupported(): boolean {
    return typeof navigator !== "undefined" && "hid" in navigator;
  }

  get controllerSlot(): number {
    return this._controllerSlot;
  }

  set controllerSlot(slot: number) {
    this._controllerSlot = Math.max(0, Math.min(3, slot));
  }

  async getDevices(showAll = false): Promise<HIDDevice[]> {
    if (!FlightHidService.isSupported()) return [];
    try {
      const all = await navigator.hid.getDevices();
      for (const d of all) logDevice(d, "enum");
      return all.filter((d) => classifyDevice(d, showAll));
    } catch (e) {
      console.warn("[Flight] getDevices failed:", e);
      return [];
    }
  }

  async requestDevice(): Promise<HIDDevice[]> {
    if (!FlightHidService.isSupported()) return [];
    try {
      const devices = await navigator.hid.requestDevice({
        filters: [
          { usagePage: GENERIC_INPUT_PAGE, usage: USAGE_JOYSTICK },
          { usagePage: GENERIC_INPUT_PAGE, usage: USAGE_GAMEPAD },
          { usagePage: GENERIC_INPUT_PAGE, usage: USAGE_MULTIAXIS },
          {},
        ],
      });
      for (const d of devices) logDevice(d, "request");
      return devices;
    } catch (e) {
      console.warn("[Flight] requestDevice cancelled or failed:", e);
      return [];
    }
  }

  async startCapture(device: HIDDevice, profile: FlightProfile): Promise<boolean> {
    this.stopCapture();

    try {
      if (!device.opened) {
        await device.open();
      }

      this.device = device;
      this.profile = profile;

      const knownConfig = getDeviceConfig(device.vendorId, device.productId);
      this.reportLayout = profile.reportLayout ?? knownConfig?.layout ?? null;

      if (!this.reportLayout) {
        console.warn(`[Flight] No report layout for ${makeVidPid(device.vendorId, device.productId)}, using raw mode`);
      }

      const vidPid = makeVidPid(device.vendorId, device.productId);
      console.log(`[Flight] Opened device: ${device.productName} (${vidPid})`);

      this.boundOnInputReport = (event: HIDInputReportEvent) => {
        this.handleInputReport(event);
      };
      device.addEventListener("inputreport", this.boundOnInputReport);

      this._capturing = true;
      this.sendConnectedState(true);
      return true;
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      console.warn("[Flight] Failed to open device:", msg);
      this.device = null;
      this.profile = null;
      this._capturing = false;
      return false;
    }
  }

  stopCapture(): void {
    if (this.device && this.boundOnInputReport) {
      this.device.removeEventListener("inputreport", this.boundOnInputReport);
      this.boundOnInputReport = null;
    }

    if (this._capturing) {
      this.sendConnectedState(false);
      this._capturing = false;
      this.lastGamepadState = null;
      console.log("[Flight] Device capture stopped");
    }

    this.device = null;
    this.profile = null;
  }

  isCapturing(): boolean {
    return this._capturing;
  }

  getActiveDevice(): HIDDevice | null {
    return this.device;
  }

  onStateUpdate(listener: (state: FlightControlsState) => void): () => void {
    this.stateListeners.add(listener);
    return () => this.stateListeners.delete(listener);
  }

  onGamepadState(listener: (state: FlightGamepadState) => void): () => void {
    this.gamepadListeners.add(listener);
    return () => this.gamepadListeners.delete(listener);
  }

  dispose(): void {
    this.stopCapture();
    this.stateListeners.clear();
    this.gamepadListeners.clear();
  }

  private handleInputReport(event: HIDInputReportEvent): void {
    const data = event.data;

    if (!this.reportLayout || !this.profile) {
      const rawBytes: number[] = [];
      for (let i = 0; i < data.byteLength; i++) {
        rawBytes.push(data.getUint8(i));
      }
      const state: FlightControlsState = {
        connected: true,
        deviceName: this.device?.productName ?? "",
        axes: [],
        buttons: [],
        hatSwitch: -1,
        rawBytes,
      };
      this.emitStateUpdate(state);
      return;
    }

    const parsed = this.parseReport(data, this.reportLayout);

    const rawBytes: number[] = [];
    for (let i = 0; i < data.byteLength; i++) {
      rawBytes.push(data.getUint8(i));
    }

    const rawState: FlightControlsState = {
      connected: true,
      deviceName: this.device?.productName ?? "",
      axes: parsed.axes,
      buttons: parsed.buttons,
      hatSwitch: parsed.hatSwitch,
      rawBytes,
    };
    this.emitStateUpdate(rawState);

    const gamepadState = this.mapToGamepad(parsed, this.profile);
    if (this.hasGamepadStateChanged(gamepadState)) {
      this.lastGamepadState = gamepadState;
      this.emitGamepadState(gamepadState);
    }
  }

  private parseReport(data: DataView, layout: FlightHidReportLayout): ParsedReport {
    const axes: number[] = [];
    for (const axisDef of layout.axes) {
      const byteIdx = axisDef.byteOffset;
      if (byteIdx + axisDef.byteCount > data.byteLength) {
        axes.push(0);
        continue;
      }

      let rawValue: number;
      if (axisDef.byteCount === 2) {
        rawValue = axisDef.littleEndian
          ? data.getUint16(byteIdx, true)
          : data.getUint16(byteIdx, false);
        if (!axisDef.unsigned && rawValue > 32767) {
          rawValue = rawValue - 65536;
        }
      } else {
        rawValue = data.getUint8(byteIdx);
        if (!axisDef.unsigned && rawValue > 127) {
          rawValue = rawValue - 256;
        }
      }

      const range = axisDef.rangeMax - axisDef.rangeMin;
      const normalized = range > 0 ? (rawValue - axisDef.rangeMin) / range : 0;
      axes.push(Math.max(0, Math.min(1, normalized)));
    }

    const buttons: boolean[] = [];
    for (const btnDef of layout.buttons) {
      const byteIdx = btnDef.byteOffset;
      if (byteIdx >= data.byteLength) {
        buttons.push(false);
        continue;
      }
      const byte = data.getUint8(byteIdx);
      buttons.push((byte & (1 << btnDef.bitIndex)) !== 0);
    }

    let hatSwitch = -1;
    if (layout.hat) {
      const byteIdx = layout.hat.byteOffset;
      if (byteIdx < data.byteLength) {
        const byte = data.getUint8(byteIdx);
        const hatValue = (byte >> layout.hat.bitOffset) & ((1 << layout.hat.bitCount) - 1);
        hatSwitch = hatValue === layout.hat.centerValue ? -1 : hatValue;
      }
    }

    return { axes, buttons, hatSwitch };
  }

  private mapToGamepad(parsed: ParsedReport, profile: FlightProfile): FlightGamepadState {
    const state: FlightGamepadState = {
      controllerId: this._controllerSlot,
      buttons: 0,
      leftTrigger: 0,
      rightTrigger: 0,
      leftStickX: 0,
      leftStickY: 0,
      rightStickX: 0,
      rightStickY: 0,
      connected: true,
    };

    for (const mapping of profile.axisMappings) {
      if (mapping.sourceIndex >= parsed.axes.length) continue;
      const rawNormalized = parsed.axes[mapping.sourceIndex]!;

      let value: number;
      if (mapping.target === "leftTrigger" || mapping.target === "rightTrigger") {
        value = mapping.inverted ? 1 - rawNormalized : rawNormalized;
        value = this.applyDeadzone(value, mapping.deadzone);
        value = this.applyCurve(value, mapping.sensitivity, mapping.curve);
        const clamped = Math.max(0, Math.min(255, Math.round(value * 255)));
        if (mapping.target === "leftTrigger") state.leftTrigger = clamped;
        else state.rightTrigger = clamped;
      } else {
        value = (rawNormalized * 2) - 1;
        if (mapping.inverted) value = -value;
        value = this.applyStickDeadzone(value, mapping.deadzone);
        value = this.applyStickCurve(value, mapping.sensitivity, mapping.curve);
        const clamped = Math.max(-32768, Math.min(32767, Math.round(value * 32767)));
        switch (mapping.target) {
          case "leftStickX": state.leftStickX = clamped; break;
          case "leftStickY": state.leftStickY = clamped; break;
          case "rightStickX": state.rightStickX = clamped; break;
          case "rightStickY": state.rightStickY = clamped; break;
        }
      }
    }

    for (const mapping of profile.buttonMappings) {
      if (mapping.sourceIndex >= parsed.buttons.length) continue;
      if (parsed.buttons[mapping.sourceIndex]) {
        state.buttons |= mapping.targetButton;
      }
    }

    if (parsed.hatSwitch >= 0) {
      const hat = parsed.hatSwitch;
      if (hat === 0 || hat === 1 || hat === 7) state.buttons |= GAMEPAD_DPAD_UP;
      if (hat === 1 || hat === 2 || hat === 3) state.buttons |= GAMEPAD_DPAD_RIGHT;
      if (hat === 3 || hat === 4 || hat === 5) state.buttons |= GAMEPAD_DPAD_DOWN;
      if (hat === 5 || hat === 6 || hat === 7) state.buttons |= GAMEPAD_DPAD_LEFT;
    }

    return state;
  }

  private applyDeadzone(value: number, deadzone: number): number {
    if (value < deadzone) return 0;
    return (value - deadzone) / (1 - deadzone);
  }

  private applyStickDeadzone(value: number, deadzone: number): number {
    const abs = Math.abs(value);
    if (abs < deadzone) return 0;
    const sign = value >= 0 ? 1 : -1;
    return sign * ((abs - deadzone) / (1 - deadzone));
  }

  private applyCurve(value: number, sensitivity: number, curve: string): number {
    if (curve === "expo") {
      return Math.pow(value, 2) * sensitivity;
    }
    return value * sensitivity;
  }

  private applyStickCurve(value: number, sensitivity: number, curve: string): number {
    const sign = value >= 0 ? 1 : -1;
    const abs = Math.abs(value);
    if (curve === "expo") {
      return sign * Math.pow(abs, 2) * sensitivity;
    }
    return value * sensitivity;
  }

  private hasGamepadStateChanged(newState: FlightGamepadState): boolean {
    if (!this.lastGamepadState) return true;
    const prev = this.lastGamepadState;
    return (
      prev.buttons !== newState.buttons ||
      prev.leftTrigger !== newState.leftTrigger ||
      prev.rightTrigger !== newState.rightTrigger ||
      prev.leftStickX !== newState.leftStickX ||
      prev.leftStickY !== newState.leftStickY ||
      prev.rightStickX !== newState.rightStickX ||
      prev.rightStickY !== newState.rightStickY
    );
  }

  private sendConnectedState(connected: boolean): void {
    const state: FlightControlsState = {
      connected,
      deviceName: this.device?.productName ?? "",
      axes: [],
      buttons: [],
      hatSwitch: -1,
      rawBytes: [],
    };
    this.emitStateUpdate(state);

    if (!connected) {
      const disconnectState: FlightGamepadState = {
        controllerId: this._controllerSlot,
        buttons: 0,
        leftTrigger: 0,
        rightTrigger: 0,
        leftStickX: 0,
        leftStickY: 0,
        rightStickX: 0,
        rightStickY: 0,
        connected: false,
      };
      this.emitGamepadState(disconnectState);
    }
  }

  private emitStateUpdate(state: FlightControlsState): void {
    for (const listener of this.stateListeners) {
      listener(state);
    }
  }

  private emitGamepadState(state: FlightGamepadState): void {
    for (const listener of this.gamepadListeners) {
      listener(state);
    }
  }
}

let instance: FlightHidService | null = null;

export function getFlightHidService(): FlightHidService {
  if (!instance) {
    instance = new FlightHidService();
  }
  return instance;
}
