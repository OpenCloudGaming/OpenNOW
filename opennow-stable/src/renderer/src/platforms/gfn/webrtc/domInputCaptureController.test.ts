/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import { INPUT_MOUSE_REL, InputEncoder } from "../inputProtocol";
import { DomInputCaptureController } from "./domInputCaptureController";

type Listener = (event: Record<string, unknown>) => void;

class FakeEventTarget {
  readonly listeners = new Map<string, Set<Listener>>();
  parentElement: FakeEventTarget | null = null;
  tabIndex = 0;

  addEventListener(type: string, listener: EventListenerOrEventListenerObject | Listener): void {
    if (typeof listener !== "function") {
      return;
    }
    let listeners = this.listeners.get(type);
    if (!listeners) {
      listeners = new Set();
      this.listeners.set(type, listeners);
    }
    listeners.add(listener as Listener);
  }

  removeEventListener(type: string, listener: EventListenerOrEventListenerObject | Listener): void {
    if (typeof listener === "function") {
      this.listeners.get(type)?.delete(listener as Listener);
    }
  }

  dispatch(type: string, event: Record<string, unknown>): void {
    for (const listener of this.listeners.get(type) ?? []) {
      listener(event);
    }
  }

  getAttribute(): string | null {
    return null;
  }

  setAttribute(): void {}

  removeAttribute(): void {}

  focus(): void {}

  getBoundingClientRect(): DOMRect {
    return {
      x: 0,
      y: 0,
      left: 0,
      top: 0,
      right: 4,
      bottom: 4,
      width: 4,
      height: 4,
      toJSON: () => ({}),
    };
  }
}

function installMouseHarness(options: {
  mouseSensitivity: number;
  resolution: string;
}): {
  controller: DomInputCaptureController;
  dispatchMouseMove: (movementX: number, timeStamp: number) => void;
  dispatchMouseDown: (clientX: number, clientY: number, timeStamp: number) => void;
  dispatchKey: (type: "keydown" | "keyup", code: string, keyCode: number, capsLock: boolean) => void;
  setPointerLocked: (locked: boolean) => void;
  pendingTimerCount: () => number;
  runNextTimer: (nowMs: number) => void;
  reliablePayloads: Uint8Array[];
  reliableSinglePayloads: Uint8Array[];
  sentInputTypes: number[];
  restoreGlobals: () => void;
} {
  const originalWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const originalDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  const originalPointerEvent = Object.getOwnPropertyDescriptor(globalThis, "PointerEvent");
  const originalPerformance = Object.getOwnPropertyDescriptor(globalThis, "performance");
  const windowTarget = new FakeEventTarget();
  const documentTarget = new FakeEventTarget();
  const videoElement = new FakeEventTarget();
  const pointerLockTarget = new FakeEventTarget();
  videoElement.parentElement = pointerLockTarget;

  let nowMs = 0;
  let nextTimerId = 1;
  const timers = new Map<number, () => void>();
  const fakeWindow = Object.assign(windowTarget, {
    setTimeout(callback: () => void): number {
      const id = nextTimerId++;
      timers.set(id, callback);
      return id;
    },
    clearTimeout(id: number): void {
      timers.delete(id);
    },
  });
  const fakeDocument = Object.assign(documentTarget, {
    body: { dataset: {} },
    documentElement: new FakeEventTarget(),
    fullscreenElement: null,
    pointerLockElement: pointerLockTarget,
    visibilityState: "visible",
    hasFocus: () => true,
    contains: () => false,
  });

  Object.defineProperty(globalThis, "window", { configurable: true, value: fakeWindow });
  Object.defineProperty(globalThis, "document", { configurable: true, value: fakeDocument });
  Object.defineProperty(globalThis, "PointerEvent", { configurable: true, value: undefined });
  Object.defineProperty(globalThis, "performance", {
    configurable: true,
    value: { now: () => nowMs },
  });

  const reliablePayloads: Uint8Array[] = [];
  const reliableSinglePayloads: Uint8Array[] = [];
  const sentInputTypes: number[] = [];
  const controller = new DomInputCaptureController(
    {
      videoElement: videoElement as unknown as HTMLVideoElement,
      inputEncoder: new InputEncoder(),
      isInputReady: () => true,
      isInputBlocked: () => false,
      isNativeInputActive: () => false,
      isNativeElectronInputBridge: () => false,
      shouldAutoFullscreen: () => false,
      getCurrentResolution: () => options.resolution,
      getKeyboardLayout: () => undefined,
      getMicState: () => "disabled",
      setWindowInputPaused: () => {},
      recordSchedulingDelay: () => {},
      refreshClipboardAvailability: async () => false,
      sendReliableSingleInput: (payload) => reliableSinglePayloads.push(payload),
      sendReliable: (payload) => reliablePayloads.push(payload),
      sendInputPacket: (_payload, inputType) => {
        sentInputTypes.push(inputType);
      },
      onGamepadConnected: () => {},
      onGamepadDisconnected: () => {},
      log: () => {},
    },
    {
      mouseSensitivity: options.mouseSensitivity,
      mouseAccelerationPercent: 1,
      nativeCursorOverlay: false,
    },
  );
  controller.install(videoElement as unknown as HTMLVideoElement);

  return {
    controller,
    dispatchMouseMove: (movementX, timeStamp) => {
      windowTarget.dispatch("mousemove", {
        movementX,
        movementY: 0,
        clientX: 0,
        clientY: 0,
        timeStamp,
      });
    },
    dispatchMouseDown: (clientX, clientY, timeStamp) => {
      pointerLockTarget.dispatch("mousedown", {
        button: 0,
        clientX,
        clientY,
        timeStamp,
        preventDefault: () => {},
      });
    },
    dispatchKey: (type, code, keyCode, capsLock) => {
      documentTarget.dispatch(type, {
        code,
        key: code === "CapsLock" ? "CapsLock" : code,
        keyCode,
        location: 0,
        repeat: false,
        shiftKey: false,
        ctrlKey: false,
        altKey: false,
        metaKey: false,
        timeStamp: 1,
        getModifierState: (modifier: string) => modifier === "CapsLock" && capsLock,
        preventDefault: () => {},
      });
    },
    setPointerLocked: (locked) => {
      (fakeDocument as { pointerLockElement: FakeEventTarget | null }).pointerLockElement = locked
        ? pointerLockTarget
        : null;
      documentTarget.dispatch("pointerlockchange", {});
    },
    pendingTimerCount: () => timers.size,
    runNextTimer: (timerNowMs) => {
      const entry = timers.entries().next().value;
      assert.ok(entry);
      const [id, callback] = entry;
      timers.delete(id);
      nowMs = timerNowMs;
      callback();
    },
    reliablePayloads,
    reliableSinglePayloads,
    sentInputTypes,
    restoreGlobals: () => {
      for (const [key, descriptor] of [
        ["window", originalWindow],
        ["document", originalDocument],
        ["PointerEvent", originalPointerEvent],
        ["performance", originalPerformance],
      ] as const) {
        if (descriptor) {
          Object.defineProperty(globalThis, key, descriptor);
        } else {
          Reflect.deleteProperty(globalThis, key);
        }
      }
    },
  };
}

test("Caps Lock never emits synthetic Shift packets", () => {
  const harness = installMouseHarness({ mouseSensitivity: 1, resolution: "1920x1080" });
  try {
    harness.dispatchKey("keydown", "CapsLock", 0x14, false);
    harness.dispatchKey("keyup", "CapsLock", 0x14, true);

    const keyPackets = harness.reliableSinglePayloads.filter((payload) => payload.byteLength === 18);
    assert.equal(keyPackets.length, 2);
    assert.deepEqual(
      keyPackets.map((payload) => ({
        type: new DataView(payload.buffer, payload.byteOffset, payload.byteLength).getUint32(0, true),
        virtualKey: new DataView(payload.buffer, payload.byteOffset, payload.byteLength).getUint16(4, false),
      })),
      [
        { type: 3, virtualKey: 0x14 },
        { type: 4, virtualKey: 0x14 },
      ],
    );
  } finally {
    harness.restoreGlobals();
  }
});

test("negative half-pixel residual parks without synchronous recursion and resumes on input", () => {
  const harness = installMouseHarness({ mouseSensitivity: 0.5, resolution: "4x4" });
  try {
    harness.dispatchMouseMove(-1, 1);
    assert.equal(harness.pendingTimerCount(), 1);
    assert.doesNotThrow(() => harness.runNextTimer(16));
    assert.deepEqual(harness.sentInputTypes, []);
    assert.equal(harness.pendingTimerCount(), 0);
    assert.equal(harness.controller.getMouseDiagnostics().residualMagnitude, 0.5);

    harness.dispatchMouseMove(-1, 2);
    assert.deepEqual(harness.sentInputTypes, [INPUT_MOUSE_REL]);
    assert.equal(harness.controller.getMouseDiagnostics().residualMagnitude, 0);
  } finally {
    harness.restoreGlobals();
  }
});

test("scaled-to-zero residual parks without synchronous recursion and resumes on input", () => {
  const harness = installMouseHarness({ mouseSensitivity: 1, resolution: "1x1" });
  try {
    harness.dispatchMouseMove(1, 1);
    assert.equal(harness.pendingTimerCount(), 1);
    assert.doesNotThrow(() => harness.runNextTimer(16));
    assert.deepEqual(harness.sentInputTypes, []);
    assert.equal(harness.pendingTimerCount(), 0);
    assert.equal(harness.controller.getMouseDiagnostics().residualMagnitude, 1);

    harness.dispatchMouseMove(3, 2);
    assert.deepEqual(harness.sentInputTypes, [INPUT_MOUSE_REL]);
    assert.equal(harness.controller.getMouseDiagnostics().residualMagnitude, 0);
  } finally {
    harness.restoreGlobals();
  }
});

test("Escape pointer-lock loss keeps the first in-game click immediately usable", () => {
  const harness = installMouseHarness({ mouseSensitivity: 1, resolution: "1920x1080" });
  try {
    harness.setPointerLocked(false);
    harness.dispatchMouseDown(2, 3, 10);

    assert.equal(harness.reliablePayloads.length, 1, "absolute cursor pin precedes the click");
    assert.equal(harness.reliableSinglePayloads.length, 1, "the click is forwarded without recapture");
    assert.deepEqual(harness.sentInputTypes, [], "the cursor pin stays ordered on the reliable channel");
  } finally {
    harness.restoreGlobals();
  }
});
