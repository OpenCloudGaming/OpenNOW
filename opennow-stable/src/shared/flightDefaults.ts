import type {
  FlightHidReportLayout,
  FlightAxisMapping,
  FlightButtonMapping,
  FlightProfile,
} from "./gfn";

import {
  GAMEPAD_A,
  GAMEPAD_B,
  GAMEPAD_X,
  GAMEPAD_Y,
  GAMEPAD_START,
  GAMEPAD_BACK,
  GAMEPAD_LSHOULDER,
  GAMEPAD_RSHOULDER,
} from "./flightConstants";

export interface KnownDeviceConfig {
  name: string;
  layout: FlightHidReportLayout;
  axisMappings: FlightAxisMapping[];
  buttonMappings: FlightButtonMapping[];
}

const THRUSTMASTER_HOTAS_ONE_LAYOUT: FlightHidReportLayout = {
  skipReportId: true,
  reportLength: 14,
  axes: [
    { byteOffset: 0, byteCount: 2, littleEndian: true, unsigned: true, rangeMin: 0, rangeMax: 65535 },
    { byteOffset: 2, byteCount: 2, littleEndian: true, unsigned: true, rangeMin: 0, rangeMax: 65535 },
    { byteOffset: 4, byteCount: 1, littleEndian: true, unsigned: true, rangeMin: 0, rangeMax: 255 },
    { byteOffset: 5, byteCount: 1, littleEndian: true, unsigned: true, rangeMin: 0, rangeMax: 255 },
    { byteOffset: 6, byteCount: 1, littleEndian: true, unsigned: true, rangeMin: 0, rangeMax: 255 },
  ],
  buttons: [
    { byteOffset: 8, bitIndex: 0 },
    { byteOffset: 8, bitIndex: 1 },
    { byteOffset: 8, bitIndex: 2 },
    { byteOffset: 8, bitIndex: 3 },
    { byteOffset: 8, bitIndex: 4 },
    { byteOffset: 8, bitIndex: 5 },
    { byteOffset: 8, bitIndex: 6 },
    { byteOffset: 8, bitIndex: 7 },
    { byteOffset: 9, bitIndex: 0 },
    { byteOffset: 9, bitIndex: 1 },
    { byteOffset: 9, bitIndex: 2 },
    { byteOffset: 9, bitIndex: 3 },
    { byteOffset: 9, bitIndex: 4 },
    { byteOffset: 9, bitIndex: 5 },
    { byteOffset: 9, bitIndex: 6 },
    { byteOffset: 9, bitIndex: 7 },
  ],
  hat: { byteOffset: 7, bitOffset: 0, bitCount: 4, centerValue: 8 },
};

const THRUSTMASTER_HOTAS_ONE_AXES: FlightAxisMapping[] = [
  { sourceIndex: 0, target: "leftStickX", inverted: false, deadzone: 0.05, sensitivity: 1.0, curve: "linear" },
  { sourceIndex: 1, target: "leftStickY", inverted: true, deadzone: 0.05, sensitivity: 1.0, curve: "linear" },
  { sourceIndex: 2, target: "rightStickX", inverted: false, deadzone: 0.08, sensitivity: 1.0, curve: "linear" },
  { sourceIndex: 3, target: "rightTrigger", inverted: true, deadzone: 0.0, sensitivity: 1.0, curve: "linear" },
];

const THRUSTMASTER_HOTAS_ONE_BUTTONS: FlightButtonMapping[] = [
  { sourceIndex: 0, targetButton: GAMEPAD_A },
  { sourceIndex: 1, targetButton: GAMEPAD_B },
  { sourceIndex: 2, targetButton: GAMEPAD_X },
  { sourceIndex: 3, targetButton: GAMEPAD_Y },
  { sourceIndex: 4, targetButton: GAMEPAD_LSHOULDER },
  { sourceIndex: 5, targetButton: GAMEPAD_RSHOULDER },
  { sourceIndex: 6, targetButton: GAMEPAD_BACK },
  { sourceIndex: 7, targetButton: GAMEPAD_START },
];

const LOGITECH_X52_LAYOUT: FlightHidReportLayout = {
  skipReportId: true,
  reportLength: 14,
  axes: [
    { byteOffset: 0, byteCount: 2, littleEndian: true, unsigned: true, rangeMin: 0, rangeMax: 65535 },
    { byteOffset: 2, byteCount: 2, littleEndian: true, unsigned: true, rangeMin: 0, rangeMax: 65535 },
    { byteOffset: 4, byteCount: 2, littleEndian: true, unsigned: true, rangeMin: 0, rangeMax: 65535 },
    { byteOffset: 6, byteCount: 1, littleEndian: true, unsigned: true, rangeMin: 0, rangeMax: 255 },
    { byteOffset: 7, byteCount: 1, littleEndian: true, unsigned: true, rangeMin: 0, rangeMax: 255 },
    { byteOffset: 8, byteCount: 1, littleEndian: true, unsigned: true, rangeMin: 0, rangeMax: 255 },
  ],
  buttons: [
    { byteOffset: 10, bitIndex: 0 },
    { byteOffset: 10, bitIndex: 1 },
    { byteOffset: 10, bitIndex: 2 },
    { byteOffset: 10, bitIndex: 3 },
    { byteOffset: 10, bitIndex: 4 },
    { byteOffset: 10, bitIndex: 5 },
    { byteOffset: 10, bitIndex: 6 },
    { byteOffset: 10, bitIndex: 7 },
    { byteOffset: 11, bitIndex: 0 },
    { byteOffset: 11, bitIndex: 1 },
    { byteOffset: 11, bitIndex: 2 },
    { byteOffset: 11, bitIndex: 3 },
    { byteOffset: 11, bitIndex: 4 },
    { byteOffset: 11, bitIndex: 5 },
    { byteOffset: 11, bitIndex: 6 },
    { byteOffset: 11, bitIndex: 7 },
    { byteOffset: 12, bitIndex: 0 },
    { byteOffset: 12, bitIndex: 1 },
    { byteOffset: 12, bitIndex: 2 },
    { byteOffset: 12, bitIndex: 3 },
    { byteOffset: 12, bitIndex: 4 },
    { byteOffset: 12, bitIndex: 5 },
    { byteOffset: 12, bitIndex: 6 },
    { byteOffset: 12, bitIndex: 7 },
    { byteOffset: 13, bitIndex: 0 },
    { byteOffset: 13, bitIndex: 1 },
    { byteOffset: 13, bitIndex: 2 },
    { byteOffset: 13, bitIndex: 3 },
    { byteOffset: 13, bitIndex: 4 },
    { byteOffset: 13, bitIndex: 5 },
    { byteOffset: 13, bitIndex: 6 },
    { byteOffset: 13, bitIndex: 7 },
  ],
  hat: { byteOffset: 9, bitOffset: 0, bitCount: 4, centerValue: 8 },
};

const LOGITECH_X52_AXES: FlightAxisMapping[] = [
  { sourceIndex: 0, target: "leftStickX", inverted: false, deadzone: 0.05, sensitivity: 1.0, curve: "linear" },
  { sourceIndex: 1, target: "leftStickY", inverted: true, deadzone: 0.05, sensitivity: 1.0, curve: "linear" },
  { sourceIndex: 2, target: "rightStickX", inverted: false, deadzone: 0.08, sensitivity: 1.0, curve: "linear" },
  { sourceIndex: 3, target: "rightTrigger", inverted: true, deadzone: 0.0, sensitivity: 1.0, curve: "linear" },
];

const LOGITECH_X52_BUTTONS: FlightButtonMapping[] = [
  { sourceIndex: 0, targetButton: GAMEPAD_A },
  { sourceIndex: 1, targetButton: GAMEPAD_B },
  { sourceIndex: 2, targetButton: GAMEPAD_X },
  { sourceIndex: 3, targetButton: GAMEPAD_Y },
  { sourceIndex: 4, targetButton: GAMEPAD_LSHOULDER },
  { sourceIndex: 5, targetButton: GAMEPAD_RSHOULDER },
  { sourceIndex: 6, targetButton: GAMEPAD_BACK },
  { sourceIndex: 7, targetButton: GAMEPAD_START },
];

const GENERIC_LAYOUT: FlightHidReportLayout = {
  skipReportId: true,
  reportLength: 8,
  axes: [
    { byteOffset: 0, byteCount: 2, littleEndian: true, unsigned: true, rangeMin: 0, rangeMax: 65535 },
    { byteOffset: 2, byteCount: 2, littleEndian: true, unsigned: true, rangeMin: 0, rangeMax: 65535 },
    { byteOffset: 4, byteCount: 1, littleEndian: true, unsigned: true, rangeMin: 0, rangeMax: 255 },
    { byteOffset: 5, byteCount: 1, littleEndian: true, unsigned: true, rangeMin: 0, rangeMax: 255 },
  ],
  buttons: [
    { byteOffset: 6, bitIndex: 0 },
    { byteOffset: 6, bitIndex: 1 },
    { byteOffset: 6, bitIndex: 2 },
    { byteOffset: 6, bitIndex: 3 },
    { byteOffset: 6, bitIndex: 4 },
    { byteOffset: 6, bitIndex: 5 },
    { byteOffset: 6, bitIndex: 6 },
    { byteOffset: 6, bitIndex: 7 },
    { byteOffset: 7, bitIndex: 0 },
    { byteOffset: 7, bitIndex: 1 },
    { byteOffset: 7, bitIndex: 2 },
    { byteOffset: 7, bitIndex: 3 },
    { byteOffset: 7, bitIndex: 4 },
    { byteOffset: 7, bitIndex: 5 },
    { byteOffset: 7, bitIndex: 6 },
    { byteOffset: 7, bitIndex: 7 },
  ],
};

const GENERIC_AXES: FlightAxisMapping[] = [
  { sourceIndex: 0, target: "leftStickX", inverted: false, deadzone: 0.05, sensitivity: 1.0, curve: "linear" },
  { sourceIndex: 1, target: "leftStickY", inverted: true, deadzone: 0.05, sensitivity: 1.0, curve: "linear" },
  { sourceIndex: 2, target: "rightStickX", inverted: false, deadzone: 0.08, sensitivity: 1.0, curve: "linear" },
  { sourceIndex: 3, target: "rightTrigger", inverted: true, deadzone: 0.0, sensitivity: 1.0, curve: "linear" },
];

const GENERIC_BUTTONS: FlightButtonMapping[] = [
  { sourceIndex: 0, targetButton: GAMEPAD_A },
  { sourceIndex: 1, targetButton: GAMEPAD_B },
  { sourceIndex: 2, targetButton: GAMEPAD_X },
  { sourceIndex: 3, targetButton: GAMEPAD_Y },
  { sourceIndex: 4, targetButton: GAMEPAD_LSHOULDER },
  { sourceIndex: 5, targetButton: GAMEPAD_RSHOULDER },
  { sourceIndex: 6, targetButton: GAMEPAD_BACK },
  { sourceIndex: 7, targetButton: GAMEPAD_START },
];

export const KNOWN_DEVICES: Record<string, KnownDeviceConfig> = {
  "044F:B67B": {
    name: "Thrustmaster T.Flight HOTAS One",
    layout: THRUSTMASTER_HOTAS_ONE_LAYOUT,
    axisMappings: THRUSTMASTER_HOTAS_ONE_AXES,
    buttonMappings: THRUSTMASTER_HOTAS_ONE_BUTTONS,
  },
  "044F:B679": {
    name: "Thrustmaster T.Flight HOTAS One (PC)",
    layout: THRUSTMASTER_HOTAS_ONE_LAYOUT,
    axisMappings: THRUSTMASTER_HOTAS_ONE_AXES,
    buttonMappings: THRUSTMASTER_HOTAS_ONE_BUTTONS,
  },
  "044F:B67D": {
    name: "Thrustmaster T.Flight HOTAS 4",
    layout: THRUSTMASTER_HOTAS_ONE_LAYOUT,
    axisMappings: THRUSTMASTER_HOTAS_ONE_AXES,
    buttonMappings: THRUSTMASTER_HOTAS_ONE_BUTTONS,
  },
  "06A3:0762": {
    name: "Logitech X52",
    layout: LOGITECH_X52_LAYOUT,
    axisMappings: LOGITECH_X52_AXES,
    buttonMappings: LOGITECH_X52_BUTTONS,
  },
  "06A3:0255": {
    name: "Logitech X52 Pro",
    layout: LOGITECH_X52_LAYOUT,
    axisMappings: LOGITECH_X52_AXES,
    buttonMappings: LOGITECH_X52_BUTTONS,
  },
};

export function getDeviceConfig(vendorId: number, productId: number): KnownDeviceConfig | null {
  const key = `${vendorId.toString(16).toUpperCase().padStart(4, "0")}:${productId.toString(16).toUpperCase().padStart(4, "0")}`;
  return KNOWN_DEVICES[key] ?? null;
}

export function getGenericConfig(deviceName: string): KnownDeviceConfig {
  return {
    name: deviceName || "Generic Joystick",
    layout: GENERIC_LAYOUT,
    axisMappings: GENERIC_AXES,
    buttonMappings: GENERIC_BUTTONS,
  };
}

export function makeVidPid(vendorId: number, productId: number): string {
  return `${vendorId.toString(16).toUpperCase().padStart(4, "0")}:${productId.toString(16).toUpperCase().padStart(4, "0")}`;
}

export function buildDefaultProfile(vendorId: number, productId: number, deviceName: string): FlightProfile {
  const config = getDeviceConfig(vendorId, productId) ?? getGenericConfig(deviceName);
  return {
    name: config.name,
    vidPid: makeVidPid(vendorId, productId),
    deviceName: deviceName || config.name,
    axisMappings: config.axisMappings.map((m) => ({ ...m })),
    buttonMappings: config.buttonMappings.map((m) => ({ ...m })),
    reportLayout: config.layout,
  };
}
