import { describe, expect, it } from "vitest";

import {
  GAMEPAD_A,
  GAMEPAD_BACK,
  GAMEPAD_DPAD_LEFT,
  GAMEPAD_DPAD_UP,
  GAMEPAD_GUIDE,
  GAMEPAD_LB,
  GAMEPAD_LS,
  GAMEPAD_RB,
  GAMEPAD_RS,
  GAMEPAD_START,
  GAMEPAD_X,
  GAMEPAD_Y,
  applyDeadzone,
  mapGamepadButtons,
  mapKeyboardEvent,
  modifierFlags,
  normalizeToInt16,
  normalizeToUint8,
  readGamepadAxes,
  toMouseButton,
} from "../../../src/renderer/src/gfn/inputProtocol";

function keyboardEvent(overrides: Partial<KeyboardEvent> = {}): KeyboardEvent {
  const modifierState = new Map<string, boolean>([
    ["CapsLock", false],
    ["NumLock", false],
  ]);

  return {
    key: "",
    code: "",
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    getModifierState: (key: string) => modifierState.get(key) ?? false,
    ...overrides,
  } as KeyboardEvent;
}

function gamepadButton(value = 0): GamepadButton {
  return {
    value,
    pressed: value > 0,
    touched: value > 0,
  };
}

function gamepad(overrides: Partial<Gamepad> = {}): Gamepad {
  return {
    id: "pad",
    index: 0,
    connected: true,
    mapping: "standard",
    timestamp: 0,
    axes: [],
    buttons: [],
    vibrationActuator: null,
    hapticActuators: [],
    ...overrides,
  } as Gamepad;
}

describe("inputProtocol helpers", () => {
  it("builds modifier bitmasks including lock states", () => {
    const event = keyboardEvent({
      ctrlKey: true,
      altKey: true,
      shiftKey: true,
      metaKey: true,
      getModifierState: (key) => key === "CapsLock" || key === "NumLock",
    });

    expect(modifierFlags(event)).toBe(0x3f);
  });

  it("maps keyboard events via code map, fallback map, alphanumerics, and unsupported cases", () => {
    expect(mapKeyboardEvent(keyboardEvent({ code: "KeyA", key: "a" }))).toEqual({ vk: 0x41, scancode: 0x04 });
    expect(mapKeyboardEvent(keyboardEvent({ code: "", key: "Esc" }))).toEqual({ vk: 0x1b, scancode: 0x29 });
    expect(mapKeyboardEvent(keyboardEvent({ code: "", key: "b" }))).toEqual({ vk: 0x42, scancode: 0 });
    expect(mapKeyboardEvent(keyboardEvent({ code: "", key: "7" }))).toEqual({ vk: 0x37, scancode: 0 });
    expect(mapKeyboardEvent(keyboardEvent({ code: "", key: "ß" }))).toBeNull();
  });

  it("maps browser mouse buttons to protocol offsets", () => {
    expect(toMouseButton(0)).toBe(1);
    expect(toMouseButton(4)).toBe(5);
  });

  it("applies radial deadzones and rescales values", () => {
    expect(applyDeadzone(0.1, 0.1, 0.2)).toEqual({ x: 0, y: 0 });
    expect(applyDeadzone(0.2, 0, 0.2)).toEqual({ x: 0, y: 0 });
    const scaled = applyDeadzone(0.5, 0, 0.2);
    expect(scaled.x).toBeCloseTo(0.375, 6);
    expect(scaled.y).toBe(0);
  });

  it("normalizes int16 and uint8 values with clamping", () => {
    expect(normalizeToInt16(1)).toBe(32767);
    expect(normalizeToInt16(-2)).toBe(-32768);
    expect(normalizeToInt16(0.5)).toBe(16384);
    expect(normalizeToUint8(0.5)).toBe(128);
    expect(normalizeToUint8(2)).toBe(255);
    expect(normalizeToUint8(-1)).toBe(0);
  });

  it("maps standard gamepad buttons to xinput flags using button values", () => {
    const pad = gamepad({
      buttons: [
        gamepadButton(1),
        gamepadButton(0),
        gamepadButton(1),
        gamepadButton(1),
        gamepadButton(1),
        gamepadButton(1),
        gamepadButton(0),
        gamepadButton(0),
        gamepadButton(1),
        gamepadButton(1),
        gamepadButton(1),
        gamepadButton(1),
        gamepadButton(1),
        gamepadButton(0),
        gamepadButton(1),
        gamepadButton(0),
        gamepadButton(1),
      ],
    });

    expect(mapGamepadButtons(pad)).toBe(
      GAMEPAD_A |
        GAMEPAD_X |
        GAMEPAD_Y |
        GAMEPAD_LB |
        GAMEPAD_RB |
        GAMEPAD_BACK |
        GAMEPAD_START |
        GAMEPAD_LS |
        GAMEPAD_RS |
        GAMEPAD_DPAD_UP |
        GAMEPAD_DPAD_LEFT |
        GAMEPAD_GUIDE,
    );
  });

  it("reads sticks with deadzone and inverts y axes", () => {
    const pad = gamepad({
      axes: [0.5, 0.5, -0.5, -0.5, 0.6, 0.7],
      buttons: [gamepadButton(), gamepadButton(), gamepadButton(), gamepadButton(), gamepadButton(), gamepadButton()],
    });

    const axes = readGamepadAxes(pad);

    expect(axes.leftStickX).toBeCloseTo(0.4634517, 6);
    expect(axes.leftStickY).toBeCloseTo(-0.4634517, 6);
    expect(axes.rightStickX).toBeCloseTo(-0.4634517, 6);
    expect(axes.rightStickY).toBeCloseTo(0.4634517, 6);
    expect(axes.leftTrigger).toBe(0.6);
    expect(axes.rightTrigger).toBe(0.7);
  });

  it("falls back to trigger axes only when trigger buttons are absent", () => {
    const withButtonObjects = gamepad({
      axes: [0, 0, 0, 0, 0.9, 0.8],
      buttons: [
        gamepadButton(),
        gamepadButton(),
        gamepadButton(),
        gamepadButton(),
        gamepadButton(),
        gamepadButton(),
        gamepadButton(0),
        gamepadButton(0),
      ],
    });
    const withoutTriggerButtons = gamepad({
      axes: [0, 0, 0, 0, 0.9, 0.8],
      buttons: [gamepadButton(), gamepadButton(), gamepadButton(), gamepadButton(), gamepadButton(), gamepadButton()],
    });

    expect(readGamepadAxes(withButtonObjects)).toMatchObject({ leftTrigger: 0, rightTrigger: 0 });
    expect(readGamepadAxes(withoutTriggerButtons)).toMatchObject({ leftTrigger: 0.9, rightTrigger: 0.8 });
  });
});
