import type { KeyboardLayout } from "@shared/gfn";

import {
  INPUT_MOUSE_ABS,
  INPUT_MOUSE_REL,
  codeMap,
  lockKeysStateFromEvent,
  mapKeyboardEvent,
  modifierFlags,
  toMouseButton,
  captureTimestampUs,
  type InputEncoder,
} from "../inputProtocol";
import { FULLSCREEN_KEYBOARD_LOCK_CODES } from "../keyboardLock";
import { GfnCursorOverlayController } from "../cursorChannel";
import {
  canForwardStreamPointerInput,
  didStreamPointerLockExit,
  getStreamPointerLockTarget,
  isStreamPointerLocked,
} from "../../../lib/pointerLock";
import {
  MouseDeltaFilter,
  quantizeMouseDeltaWithResidual,
  subsampleCoalescedPointerEvents,
} from "./mouseInput";

interface DomInputCaptureDependencies {
  videoElement: HTMLVideoElement;
  inputEncoder: InputEncoder;
  isInputReady: () => boolean;
  isInputBlocked: () => boolean;
  isNativeInputActive: () => boolean;
  isNativeElectronInputBridge: () => boolean;
  shouldAutoFullscreen: () => boolean;
  getCurrentResolution: () => string;
  getKeyboardLayout: () => KeyboardLayout | undefined;
  getMicState: () => string;
  setWindowInputPaused: (paused: boolean) => void;
  recordSchedulingDelay: (delayMs: number) => void;
  refreshClipboardAvailability: () => Promise<boolean>;
  sendReliableSingleInput: (payload: Uint8Array) => void;
  sendReliable: (payload: Uint8Array) => void;
  sendInputPacket: (payload: Uint8Array, inputType: number) => void;
  onGamepadConnected: (event: GamepadEvent) => void;
  onGamepadDisconnected: (event: GamepadEvent) => void;
  log: (message: string) => void;
}

export interface MouseInputDiagnostics {
  flushBaseIntervalMs: number;
  flushIntervalMs: number;
  packetsPerSecond: number;
  residualMagnitude: number;
  adaptiveFlushActive: boolean;
}

const MOUSE_FLUSH_FAST_MS = 4;
const MOUSE_FLUSH_NORMAL_MS = 8;
const MOUSE_FLUSH_SAFE_MS = 16;

function timestampUs(sourceTimestampMs?: number): bigint {
  return captureTimestampUs(sourceTimestampMs);
}

function parseResolution(resolution: string): { width: number; height: number } {
  const [rawWidth, rawHeight] = resolution.split("x");
  const width = Number.parseInt(rawWidth ?? "", 10);
  const height = Number.parseInt(rawHeight ?? "", 10);
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    return { width: 1920, height: 1080 };
  }
  return { width, height };
}

export class DomInputCaptureController {
  private cursorOverlay: GfnCursorOverlayController | null = null;
  private inputCleanup: Array<() => void> = [];
  private readonly pressedKeys = new Set<number>();
  private pointerLockTarget: HTMLElement | null = null;
  private autoPointerLockInProgress = false;
  private pointerLockEscapeTimer: number | null = null;
  private pointerLockRelockTimer: number | null = null;
  private suppressNextSyntheticEscape = false;
  private syntheticEscapeSuppressionTimer: number | null = null;
  private keyboardLockState: "unknown" | "unsupported" | "locked" | "failed" = "unknown";
  private lastLockKeysState = -1;
  private mouseFlushTimer: number | null = null;
  private flushPendingMouseMovement: () => void = () => {};
  private pendingMouseDxFloat = 0;
  private pendingMouseDyFloat = 0;
  private pendingMouseAbs: { x: number; y: number; width: number; height: number } | null = null;
  private pendingMouseTimestampUs: bigint | null = null;
  private readonly mouseDeltaFilter = new MouseDeltaFilter();
  private mouseSensitivity = 1;
  private mouseAccelerationPercent = 1;
  private mouseFlushBaseIntervalMs = MOUSE_FLUSH_NORMAL_MS;
  private mouseFlushIntervalMs = MOUSE_FLUSH_NORMAL_MS;
  private mouseAdaptiveFlushActive = false;
  private mousePacketsSentInWindow = 0;
  private mousePacketsPerSecond = 0;
  private mousePacketRateWindowStartedAtMs = 0;
  private mouseFlushLastSendMs = 0;
  private mouseCoalescedBatchEntries = 0;
  private nativeCursorOverlayEnabled: boolean;

  constructor(
    private readonly dependencies: DomInputCaptureDependencies,
    options: { mouseSensitivity: number; mouseAccelerationPercent: number; nativeCursorOverlay: boolean },
  ) {
    this.mouseSensitivity = options.mouseSensitivity;
    this.mouseAccelerationPercent = options.mouseAccelerationPercent;
    this.nativeCursorOverlayEnabled = options.nativeCursorOverlay;
  }

  setMouseSensitivity(value: number): void {
    this.mouseSensitivity = Math.max(0.01, Number.isFinite(value) ? value : 1);
  }

  setMouseAccelerationPercent(value: number): void {
    this.mouseAccelerationPercent = Math.max(1, Math.min(150, Math.round(Number.isFinite(value) ? value : 1)));
  }

  isNativeCursorOverlayEnabled(): boolean {
    return this.nativeCursorOverlayEnabled;
  }

  setNativeCursorOverlayEnabled(enabled: boolean): void {
    this.nativeCursorOverlayEnabled = enabled;
    if (!enabled) {
      this.cursorOverlay?.dispose();
      this.cursorOverlay = null;
      return;
    }
    if (!this.cursorOverlay) {
      this.cursorOverlay = new GfnCursorOverlayController(this.dependencies.videoElement);
      this.cursorOverlay.setFallbackResolution(parseResolution(this.dependencies.getCurrentResolution()));
      const lockElement = document.pointerLockElement;
      const pointerLockTarget = this.dependencies.videoElement.parentElement;
      this.cursorOverlay.setPointerLocked(
        lockElement === this.dependencies.videoElement || lockElement === pointerLockTarget,
      );
    }
  }

  setFallbackResolution(resolution: string): void {
    this.cursorOverlay?.setFallbackResolution(parseResolution(resolution));
  }

  handleCursorMessage(bytes: Uint8Array): boolean {
    return this.cursorOverlay?.handleMessage(bytes) ?? false;
  }

  suppressNextSyntheticEscapeOnPointerLockLoss(durationMs = 1000): void {
    this.clearSyntheticEscapeSuppression();
    this.suppressNextSyntheticEscape = true;
    this.syntheticEscapeSuppressionTimer = window.setTimeout(() => {
      this.clearSyntheticEscapeSuppression();
    }, Math.max(0, durationMs));
  }

  detach(): void {
    for (const cleanup of this.inputCleanup.splice(0)) {
      cleanup();
    }
    this.cursorOverlay?.dispose();
    this.cursorOverlay = null;
    this.flushPendingMouseMovement = () => {};
  }

  flushPendingMovement(): void {
    this.flushPendingMouseMovement();
  }

  reset(): void {
    this.detach();
    if (this.mouseFlushTimer !== null) {
      window.clearTimeout(this.mouseFlushTimer);
      this.mouseFlushTimer = null;
    }
    this.clearSyntheticEscapeSuppression();
    this.pendingMouseDxFloat = 0;
    this.pendingMouseDyFloat = 0;
    this.pendingMouseAbs = null;
    this.pendingMouseTimestampUs = null;
    this.mouseDeltaFilter.reset();
    this.mouseFlushLastSendMs = 0;
    this.mouseCoalescedBatchEntries = 0;
    this.mouseFlushBaseIntervalMs = MOUSE_FLUSH_NORMAL_MS;
    this.mouseFlushIntervalMs = MOUSE_FLUSH_NORMAL_MS;
    this.mouseAdaptiveFlushActive = false;
    this.mousePacketsSentInWindow = 0;
    this.mousePacketsPerSecond = 0;
    this.mousePacketRateWindowStartedAtMs = 0;
    this.lastLockKeysState = -1;
  }

  getMouseDiagnostics(): MouseInputDiagnostics {
    return {
      flushBaseIntervalMs: this.mouseFlushBaseIntervalMs,
      flushIntervalMs: this.mouseFlushIntervalMs,
      packetsPerSecond: this.mousePacketsPerSecond,
      residualMagnitude: Math.hypot(this.pendingMouseDxFloat, this.pendingMouseDyFloat),
      adaptiveFlushActive: this.mouseAdaptiveFlushActive,
    };
  }

  setAdaptiveFlushInterval(intervalMs: number, active: boolean): void {
    this.mouseFlushIntervalMs = intervalMs;
    this.mouseAdaptiveFlushActive = active;
  }

  clearSyntheticEscapeSuppression(): void {
    this.suppressNextSyntheticEscape = false;
    if (this.syntheticEscapeSuppressionTimer !== null) {
      window.clearTimeout(this.syntheticEscapeSuppressionTimer);
      this.syntheticEscapeSuppressionTimer = null;
    }
  }

  private consumeSyntheticEscapeSuppression(): boolean {
    if (!this.suppressNextSyntheticEscape) {
      return false;
    }
    this.clearSyntheticEscapeSuppression();
    return true;
  }

  async requestPointerLockCompat(
    lockTarget: HTMLElement,
    options?: { unadjustedMovement?: boolean },
  ): Promise<void> {
    const maybePromise = lockTarget.requestPointerLock(options as any) as unknown;
    if (maybePromise && typeof (maybePromise as Promise<void>).then === "function") {
      await (maybePromise as Promise<void>);
    }
  }

  private syncLockKeysState(event: KeyboardEvent): void {
    const state = lockKeysStateFromEvent(event);
    if (state === this.lastLockKeysState) {
      return;
    }
    this.lastLockKeysState = state;
    if (!this.dependencies.isInputReady()) {
      return;
    }
    this.dependencies.sendReliableSingleInput(this.dependencies.inputEncoder.encodeLockKeysSync(state));
  }

  private requestEscapeKeyboardLock(): void {
    if (!document.fullscreenElement) {
      if (this.keyboardLockState === "locked") {
        this.keyboardLockState = "unknown";
      }
      return;
    }

    const nav = navigator as any;
    if (!nav.keyboard?.lock) {
      if (this.keyboardLockState !== "unsupported") {
        this.keyboardLockState = "unsupported";
        this.dependencies.log("Keyboard Lock API unavailable; Escape may release pointer lock");
      }
      return;
    }

    void Promise.resolve(nav.keyboard.lock(FULLSCREEN_KEYBOARD_LOCK_CODES))
      .then(() => {
        if (this.keyboardLockState !== "locked") {
          this.keyboardLockState = "locked";
          this.dependencies.log("Keyboard lock active for fullscreen stream");
        }
      })
      .catch((error: unknown) => {
        this.keyboardLockState = "failed";
        this.dependencies.log(`Keyboard Escape lock failed: ${String(error)}`);
      });
  }

  private async requestPointerLockWithOptionalFullscreen(
    lockTarget: HTMLElement,
    ensureFullscreen: boolean,
  ): Promise<void> {
    if (ensureFullscreen && !document.fullscreenElement) {
      if (typeof window.openNow?.setFullscreen === "function") {
        try {
          await window.openNow.setFullscreen(true);
        } catch (error) {
          this.dependencies.log(`Native fullscreen request failed: ${String(error)}`);
        }
      } else {
        try {
          await document.documentElement.requestFullscreen();
        } catch (error) {
          this.dependencies.log(`DOM fullscreen request failed: ${String(error)}`);
        }
      }
    }

    this.requestEscapeKeyboardLock();

    try {
      await this.requestPointerLockCompat(lockTarget, { unadjustedMovement: true });
      this.dependencies.log("Pointer lock acquired with unadjustedMovement=true (raw/unaccelerated)");
    } catch (err) {
      const domErr = err as DOMException;
      if (domErr?.name === "NotSupportedError") {
        this.dependencies.log("unadjustedMovement not supported, falling back to standard pointer lock (accelerated)");
        await this.requestPointerLockCompat(lockTarget);
      } else {
        throw err;
      }
    }
  }

  async attemptAutoPointerLock(ensureFullscreen = true): Promise<void> {
    if (this.autoPointerLockInProgress) return;
    this.autoPointerLockInProgress = true;
    try {
      const target = this.pointerLockTarget ?? this.dependencies.videoElement;
      if (!target) return;
      const lockElement = document.pointerLockElement;
      if (lockElement === target || lockElement === this.dependencies.videoElement) {
        return;
      }

      try {
        await this.requestPointerLockWithOptionalFullscreen(target, ensureFullscreen);
        this.dependencies.log("Auto pointer lock acquired");
        return;
      } catch (err) {
        // Fallback to a simpler request if the guarded method fails
        try {
          await this.requestPointerLockCompat(target, { unadjustedMovement: true });
          this.dependencies.log("Auto pointer lock acquired (fallback)");
          return;
        } catch {
          this.dependencies.log(`Auto pointer lock failed: ${String(err)}`);
        }
      }
    } finally {
      this.autoPointerLockInProgress = false;
    }
  }

  private shouldSendSyntheticEscapeOnPointerLockLoss(): boolean {
    if (document.visibilityState !== "visible") {
      return false;
    }
    if (typeof document.hasFocus === "function" && !document.hasFocus()) {
      return false;
    }
    return true;
  }

  releasePressedKeys(reason: string): void {
    if (this.pressedKeys.size === 0 || !this.dependencies.isInputReady()) {
      this.pressedKeys.clear();
      return;
    }

    this.dependencies.log(`Releasing ${this.pressedKeys.size} key(s): ${reason}`);
    for (const vk of this.pressedKeys) {
      const payload = this.dependencies.inputEncoder.encodeKeyUp({
        keycode: vk,
        scancode: 0,
        modifiers: 0,
        timestampUs: timestampUs(),
      });
      this.dependencies.sendReliableSingleInput(payload);
    }
    this.pressedKeys.clear();
  }

  private sendKeyPacket(vk: number, scancode: number, modifiers: number, isDown: boolean): void {
    const payload = isDown
      ? this.dependencies.inputEncoder.encodeKeyDown({
        keycode: vk,
        scancode,
        modifiers,
        timestampUs: timestampUs(),
      })
      : this.dependencies.inputEncoder.encodeKeyUp({
        keycode: vk,
        scancode,
        modifiers,
        timestampUs: timestampUs(),
      });
    this.dependencies.sendReliableSingleInput(payload);
  }

  public sendAntiAfkPulse(): boolean {
    if (!this.dependencies.isInputReady()) {
      return false;
    }

    this.sendKeyPacket(codeMap.F13.vk, codeMap.F13.scancode, 0, true);
    window.setTimeout(() => this.sendKeyPacket(codeMap.F13.vk, codeMap.F13.scancode, 0, false), 50);
    return true;
  }

  public sendPasteShortcut(useMeta: boolean): boolean {
    if (!this.dependencies.isInputReady()) {
      return false;
    }

    const modifier = useMeta
      ? { ...codeMap.MetaLeft, flag: 0x08 }
      : { ...codeMap.ControlLeft, flag: 0x02 };

    this.sendKeyPacket(modifier.vk, modifier.scancode, modifier.flag, true);
    this.sendKeyPacket(codeMap.KeyV.vk, codeMap.KeyV.scancode, modifier.flag, true);
    this.sendKeyPacket(codeMap.KeyV.vk, codeMap.KeyV.scancode, modifier.flag, false);
    this.sendKeyPacket(modifier.vk, modifier.scancode, 0, false);
    return true;
  }

  public sendText(text: string): number {
    if (!this.dependencies.isInputReady() || !text) {
      return 0;
    }

    const chunks = this.dependencies.inputEncoder.encodeTextInput(text);
    for (const chunk of chunks) {
      this.dependencies.sendReliable(chunk);
    }

    return Array.from(text).length;
  }

  install(videoElement: HTMLVideoElement): void {
    this.detach();

    const pointerLockTarget = getStreamPointerLockTarget(videoElement);
    const originalPointerLockTargetTabIndex = pointerLockTarget.getAttribute("tabindex");
    if (this.isNativeCursorOverlayEnabled()) {
      this.cursorOverlay = new GfnCursorOverlayController(videoElement);
      this.cursorOverlay.setFallbackResolution(parseResolution(this.dependencies.getCurrentResolution()));
    } else {
      this.cursorOverlay = null;
    }
    if (originalPointerLockTargetTabIndex === null) {
      pointerLockTarget.tabIndex = -1;
    }
    const focusPointerLockTarget = (): void => {
      try {
        pointerLockTarget.focus({ preventScroll: true });
      } catch {
        pointerLockTarget.focus();
      }
    };
    const isPointerLockActive = (): boolean => {
      return isStreamPointerLocked(videoElement);
    };
    let pointerLockWasActive = isPointerLockActive();
    this.cursorOverlay?.setPointerLocked(pointerLockWasActive);

    // Mirror mode: tracks whether the HW cursor is over the stream viewport.
    // Dual-source: coarse window focus/blur sets the initial state and handles
    // cases where the cursor was already inside when the stream started;
    // mouseenter/mouseleave on pointerLockTarget refines it for sub-window
    // boundaries (overlays, toolbars, multi-monitor cursor exit without blur).
    let mouseInStreamView = document.hasFocus();
    let lastAbsX: number | null = null;
    let lastAbsY: number | null = null;
    // Prevent repeated auto-lock attempts within the same focus session.
    let autoLockPending = false;
    let escapePointerFallbackActive = false;

    // Track an approximate server-side absolute pointer position (in server
    // pixels — the remote stream's resolution) so we can align the server cursor
    // to the hardware cursor when transitioning from mirror -> pointer-lock.
    // `null` means unknown; when unknown we assume server cursor equals HW cursor on first entry.
    let simulatedAbsX: number | null = null;
    let simulatedAbsY: number | null = null;
    // When a document-level entry event triggers tryAutoLock, we store the
    // entry absolute coordinates here so tryAutoLock can align before locking.
    let pendingEntryAbsX: number | null = null;
    let pendingEntryAbsY: number | null = null;

    const onPointerLockTargetMouseEnter = (): void => {
      mouseInStreamView = true;
      lastAbsX = null;
      lastAbsY = null;
      tryAutoLock();
    };

    const onPointerLockTargetMouseLeave = (): void => {
      mouseInStreamView = false;
      lastAbsX = null;
      lastAbsY = null;
      autoLockPending = false;
    };

    const hasPointerRawUpdate = "onpointerrawupdate" in videoElement;
    const hasCoalescedEvents =
      typeof PointerEvent !== "undefined" && "getCoalescedEvents" in PointerEvent.prototype;
    const pointerMoveEventName: "pointerrawupdate" | "pointermove" | null = hasPointerRawUpdate
      ? "pointerrawupdate"
      : (typeof PointerEvent !== "undefined" ? "pointermove" : null);
    this.mouseFlushBaseIntervalMs = hasPointerRawUpdate
      ? MOUSE_FLUSH_FAST_MS
      : hasCoalescedEvents
        ? MOUSE_FLUSH_NORMAL_MS
        : MOUSE_FLUSH_SAFE_MS;
    this.mouseFlushIntervalMs = this.mouseFlushBaseIntervalMs;
    this.mouseAdaptiveFlushActive = false;
    const mouseInitNow = performance.now();
    this.mouseFlushLastSendMs = mouseInitNow;
    this.mouseCoalescedBatchEntries = 0;
    this.pendingMouseDxFloat = 0;
    this.pendingMouseDyFloat = 0;
    this.pendingMouseAbs = null;
    this.pendingMouseTimestampUs = null;
    this.mousePacketsPerSecond = 0;
    this.mousePacketsSentInWindow = 0;
    this.mousePacketRateWindowStartedAtMs = mouseInitNow;
    this.mouseDeltaFilter.reset();
    this.mouseDeltaFilter.setRelaxedForRawInput(hasPointerRawUpdate);
    this.dependencies.log(
      `Mouse input mode: ${pointerMoveEventName ?? "mousemove"}, coalesced=${hasCoalescedEvents ? "yes" : "no"}, flush=${this.mouseFlushIntervalMs}ms`,
    );

    const pointerScaleCache = {
      rectWidth: 0,
      rectHeight: 0,
      scaleX: 1,
      scaleY: 1,
      serverWidth: 0,
      serverHeight: 0,
      resolution: "",
    };
    const getPointerScale = (): typeof pointerScaleCache => {
      const rect = pointerLockTarget.getBoundingClientRect();
      const resolution = this.dependencies.getCurrentResolution() ?? "";
      if (
        pointerScaleCache.rectWidth === rect.width
        && pointerScaleCache.rectHeight === rect.height
        && pointerScaleCache.resolution === resolution
      ) {
        return pointerScaleCache;
      }

      let serverWidth = rect.width;
      let serverHeight = rect.height;
      const resMatch = /^([0-9]+)x([0-9]+)$/.exec(resolution);
      if (resMatch) {
        serverWidth = parseInt(resMatch[1], 10) || serverWidth;
        serverHeight = parseInt(resMatch[2], 10) || serverHeight;
      }

      pointerScaleCache.rectWidth = rect.width;
      pointerScaleCache.rectHeight = rect.height;
      pointerScaleCache.serverWidth = serverWidth;
      pointerScaleCache.serverHeight = serverHeight;
      pointerScaleCache.scaleX = rect.width > 0 ? serverWidth / rect.width : 1;
      pointerScaleCache.scaleY = rect.height > 0 ? serverHeight / rect.height : 1;
      pointerScaleCache.resolution = resolution;
      return pointerScaleCache;
    };

    const updateMousePacketRate = (): void => {
      const now = performance.now();
      if (this.mousePacketRateWindowStartedAtMs <= 0) {
        this.mousePacketRateWindowStartedAtMs = now;
      }
      const elapsed = now - this.mousePacketRateWindowStartedAtMs;
      if (elapsed >= 1000) {
        this.mousePacketsPerSecond = Math.round((this.mousePacketsSentInWindow * 1000) / elapsed);
        this.mousePacketsSentInWindow = 0;
        this.mousePacketRateWindowStartedAtMs = now;
      }
    };

    let pointerRawStuckCount = 0;
    let lastPointerClientX = Number.NaN;
    let lastPointerClientY = Number.NaN;

    const hasPendingMouseMovement = (): boolean =>
      this.pendingMouseAbs !== null
      || Math.abs(this.pendingMouseDxFloat) >= 0.5
      || Math.abs(this.pendingMouseDyFloat) >= 0.5;

    const markServerCursorAt = (abs: { x: number; y: number; width: number; height: number }): void => {
      // An absolute packet pins the server cursor exactly; keep the simulated
      // server-pixel baseline in sync for the pointer-lock entry alignment path.
      const { serverWidth, serverHeight } = getPointerScale();
      simulatedAbsX = Math.round((abs.x / abs.width) * serverWidth);
      simulatedAbsY = Math.round((abs.y / abs.height) * serverHeight);
    };

    const flushMouse = (forceReliable = false): boolean => {
      const tickNow = performance.now();
      if (!this.dependencies.isInputReady() || !hasPendingMouseMovement()) {
        return false;
      }

      // A batch can hold both an absolute position (queued while the overlay
      // cursor was visible) and relative deltas accumulated after the cursor
      // was hidden mid-batch. Send the absolute packet first, then the
      // relative deltas, preserving event order like the official client's
      // mixed batch encoding — never discard queued relative movement.
      const batchTimestampUs = this.pendingMouseTimestampUs ?? timestampUs();
      let sentAny = false;

      // Compute the relative part first (without consuming it) so a mixed
      // abs+rel pair can be detected up front. The partially reliable channel
      // is unordered, so a dependent pair must travel on the ordered reliable
      // channel or the relative delta could arrive before the absolute pin
      // and be overwritten by it.
      let relPart: {
        dxServer: number;
        dyServer: number;
        residualX: number;
        residualY: number;
      } | null = null;
      if (
        Math.abs(this.pendingMouseDxFloat) >= 0.5
        || Math.abs(this.pendingMouseDyFloat) >= 0.5
      ) {
        const { scaleX, scaleY } = getPointerScale();
        const dxQuantized = quantizeMouseDeltaWithResidual(this.pendingMouseDxFloat);
        const dyQuantized = quantizeMouseDeltaWithResidual(this.pendingMouseDyFloat);
        const dxServer = Math.max(-32768, Math.min(32767, Math.round(dxQuantized.send * scaleX)));
        const dyServer = Math.max(-32768, Math.min(32767, Math.round(dyQuantized.send * scaleY)));
        if (dxServer !== 0 || dyServer !== 0) {
          relPart = {
            dxServer,
            dyServer,
            residualX: dxQuantized.residual,
            residualY: dyQuantized.residual,
          };
        }
      }
      const mixedBatch = this.pendingMouseAbs !== null && relPart !== null;

      if (this.pendingMouseAbs !== null) {
        const abs = this.pendingMouseAbs;
        this.pendingMouseAbs = null;
        const payload = this.dependencies.inputEncoder.encodeMouseAbsolute({
          ...abs,
          timestampUs: batchTimestampUs,
        });
        if (mixedBatch || forceReliable) {
          this.dependencies.sendReliable(payload);
        } else {
          this.dependencies.sendInputPacket(payload, INPUT_MOUSE_ABS);
        }
        this.mousePacketsSentInWindow += 1;
        markServerCursorAt(abs);
        sentAny = true;
      }

      if (relPart !== null) {
        this.pendingMouseDxFloat = relPart.residualX;
        this.pendingMouseDyFloat = relPart.residualY;

        const payload = this.dependencies.inputEncoder.encodeMouseMove({
          dx: relPart.dxServer,
          dy: relPart.dyServer,
          timestampUs: batchTimestampUs,
        });
        if (mixedBatch || forceReliable) {
          this.dependencies.sendReliable(payload);
        } else {
          this.dependencies.sendInputPacket(payload, INPUT_MOUSE_REL);
        }
        this.mousePacketsSentInWindow += 1;

        if (simulatedAbsX !== null && simulatedAbsY !== null) {
          simulatedAbsX += relPart.dxServer;
          simulatedAbsY += relPart.dyServer;
        }
        sentAny = true;
      }

      if (!sentAny) {
        return false;
      }

      const expectedSendAt = this.mouseFlushLastSendMs + this.mouseFlushIntervalMs;
      this.dependencies.recordSchedulingDelay(Math.max(0, tickNow - expectedSendAt));
      this.pendingMouseTimestampUs = null;
      this.mouseCoalescedBatchEntries = 0;
      this.mouseFlushLastSendMs = tickNow;
      updateMousePacketRate();
      return true;
    };

    this.flushPendingMouseMovement = () => {
      try {
        flushMouse();
      } catch (err) {
        this.dependencies.log(`Mouse flush failed (non-fatal): ${String(err)}`);
      }
    };

    /** Official GFN dl(): schedule cl() after the coalesce interval elapses. */
    const scheduleMouseBatchFlush = (): void => {
      if (this.mouseFlushTimer !== null) {
        return;
      }

      const now = performance.now();
      const elapsed = now - this.mouseFlushLastSendMs;
      if (this.mouseFlushIntervalMs <= 0 || elapsed >= this.mouseFlushIntervalMs) {
        flushMouse();
        return;
      }

      this.mouseFlushTimer = window.setTimeout(() => {
        this.mouseFlushTimer = null;
        try {
          flushMouse();
        } catch (err) {
          this.dependencies.log(`Mouse flush tick failed (non-fatal): ${String(err)}`);
        }
      }, Math.max(0, this.mouseFlushIntervalMs - elapsed));
    };

    /** Official GFN Cp(): flush on empty -> non-empty, or when new input makes a parked residual sendable. */
    const afterPointerMovement = (): void => {
      if (!hasPendingMouseMovement()) {
        return;
      }
      const elapsed = performance.now() - this.mouseFlushLastSendMs;
      if (this.mouseFlushIntervalMs <= 0 || elapsed >= this.mouseFlushIntervalMs) {
        flushMouse();
      } else {
        scheduleMouseBatchFlush();
      }
    };

    const queueUnlockedAbsolutePointer = (
      event: MouseEvent | PointerEvent,
      flushAfterQueue = true,
    ): void => {
      const rect = pointerLockTarget.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) {
        return;
      }

      const x = Math.max(0, Math.min(rect.width, event.clientX - rect.left));
      const y = Math.max(0, Math.min(rect.height, event.clientY - rect.top));
      this.cursorOverlay?.setClientPosition(rect.left + x, rect.top + y);
      this.pendingMouseAbs = this.cursorOverlay?.isCursorVisible()
        ? this.cursorOverlay.getAbsolutePosition()
        : { x, y, width: rect.width, height: rect.height };
      this.pendingMouseTimestampUs = timestampUs(event.timeStamp);
      this.mouseCoalescedBatchEntries += 1;
      if (flushAfterQueue) {
        afterPointerMovement();
      }
    };

    const tryAutoLock = (): void => {
      try {
        if (document?.body?.dataset?.sidebarOpen === "1") {
          return;
        }
      } catch {}

      if (autoLockPending || isPointerLockActive() || !mouseInStreamView || !this.dependencies.isInputReady()) {
        return;
      }
      autoLockPending = true;

      // Align server cursor to current HW cursor (if we have an entry position)
      // before requesting pointer lock so the transition appears smooth.
      try {
        const targetAbsX = pendingEntryAbsX ?? lastAbsX;
        const targetAbsY = pendingEntryAbsY ?? lastAbsY;
        // Consume pending entry coords
        pendingEntryAbsX = null;
        pendingEntryAbsY = null;

        if (typeof targetAbsX === "number" && typeof targetAbsY === "number") {
          const targetRect = pointerLockTarget.getBoundingClientRect();
          this.cursorOverlay?.setClientPosition(targetRect.left + targetAbsX, targetRect.top + targetAbsY);
          const overlayAbs = this.cursorOverlay?.isCursorVisible()
            ? this.cursorOverlay.getAbsolutePosition()
            : null;
          const { scaleX, scaleY, serverWidth, serverHeight } = getPointerScale();

          if (overlayAbs) {
            // Overlay cursor is visible: pin the server cursor with one
            // absolute packet instead of simulating relative moves.
            const movePayload = this.dependencies.inputEncoder.encodeMouseAbsolute({
              ...overlayAbs,
              timestampUs: timestampUs(),
            });
            this.dependencies.sendReliable(movePayload);
            markServerCursorAt(overlayAbs);
          } else {
            // Translate the element-local target into server pixels.
            const targetServerX = Math.round(targetAbsX * scaleX);
            const targetServerY = Math.round(targetAbsY * scaleY);

            if (simulatedAbsX === null || simulatedAbsY === null) {
              // No baseline known: assume server cursor is centered and move from
              // center -> target in server pixels so remote cursor matches HW cursor.
              const baselineXServer = Math.round(serverWidth / 2);
              const baselineYServer = Math.round(serverHeight / 2);
              const dx = Math.round(targetServerX - baselineXServer);
              const dy = Math.round(targetServerY - baselineYServer);
              if (dx !== 0 || dy !== 0) {
                const movePayload = this.dependencies.inputEncoder.encodeMouseMove({
                  dx: Math.max(-32768, Math.min(32767, dx)),
                  dy: Math.max(-32768, Math.min(32767, dy)),
                  timestampUs: timestampUs(),
                });
                this.dependencies.sendReliable(movePayload);
              }
              // Record simulated baseline in server pixels.
              simulatedAbsX = targetServerX;
              simulatedAbsY = targetServerY;
            } else {
              // sim values are stored in server pixels now; compute server delta.
              const dx = Math.round(targetServerX - simulatedAbsX);
              const dy = Math.round(targetServerY - simulatedAbsY);
              if (dx !== 0 || dy !== 0) {
                const movePayload = this.dependencies.inputEncoder.encodeMouseMove({
                  dx: Math.max(-32768, Math.min(32767, dx)),
                  dy: Math.max(-32768, Math.min(32767, dy)),
                  timestampUs: timestampUs(),
                });
                this.dependencies.sendReliable(movePayload);
                simulatedAbsX += dx;
                simulatedAbsY += dy;
              }
            }
          }
        }
      } catch (err) {
        this.dependencies.log(`Pointer lock alignment failed (non-fatal): ${String(err)}`);
      }

      void this.attemptAutoPointerLock(this.dependencies.shouldAutoFullscreen())
        .catch(() => {})
        .finally(() => {
          autoLockPending = false;
        });
    };

    const queueMouseMovement = (dx: number, dy: number, eventTimestampMs: number): void => {
      if (!this.dependencies.isInputReady() || !isPointerLockActive()) {
        return;
      }

      if (!this.mouseDeltaFilter.update(dx, dy, eventTimestampMs)) {
        return;
      }

      // Apply user-configured sensitivity, then optional software acceleration.
      let adjustedDx = this.mouseDeltaFilter.getX() * this.mouseSensitivity;
      let adjustedDy = this.mouseDeltaFilter.getY() * this.mouseSensitivity;

      if (this.mouseAccelerationPercent > 1) {
        const speed = Math.hypot(adjustedDx, adjustedDy);
        const strength = (this.mouseAccelerationPercent - 1) / 149;
        // Gentle curve: low-speed precision, high-speed turn boost (caps at +60% at 150%).
        const accelFactor = 1 + Math.min(0.6 * strength, (speed / 50) * strength);
        adjustedDx *= accelFactor;
        adjustedDy *= accelFactor;
      }

      this.cursorOverlay?.moveBy(adjustedDx, adjustedDy);

      // Official GFN local-cursor mode: while the client-rendered cursor is
      // visible, send absolute positions (type 5) that mirror the clamped
      // overlay position so the server cursor cannot drift from the overlay.
      // Relative deltas (type 7) remain for hidden-cursor/raw-input games.
      if (this.cursorOverlay?.isCursorVisible()) {
        const abs = this.cursorOverlay.getAbsolutePosition();
        if (abs) {
          // Deliver raw-input deltas queued before the cursor became
          // visible ahead of the absolute pin, in order, on the reliable
          // channel — never after it, where they would shift the server
          // cursor off the overlay.
          if (
            Math.abs(this.pendingMouseDxFloat) >= 0.5
            || Math.abs(this.pendingMouseDyFloat) >= 0.5
          ) {
            flushMouse(true);
          }
          this.pendingMouseDxFloat = 0;
          this.pendingMouseDyFloat = 0;
          this.pendingMouseAbs = abs;
          if (this.pendingMouseTimestampUs === null) {
            this.pendingMouseTimestampUs = timestampUs(eventTimestampMs);
          }
          this.mouseCoalescedBatchEntries += 1;
          return;
        }
      }

      this.pendingMouseDxFloat += adjustedDx;
      this.pendingMouseDyFloat += adjustedDy;
      if (this.pendingMouseTimestampUs === null) {
        this.pendingMouseTimestampUs = timestampUs(eventTimestampMs);
      }
      this.mouseCoalescedBatchEntries += 1;
    };

    const processRelativePointerSamples = (
      samples: readonly { movementX: number; movementY: number; timeStamp: number }[],
    ): void => {
      const hadBatch = hasPendingMouseMovement();
      const { events } = subsampleCoalescedPointerEvents(samples, this.mouseCoalescedBatchEntries);
      for (const sample of events) {
        queueMouseMovement(sample.movementX, sample.movementY, sample.timeStamp);
      }
      if (
        hasPendingMouseMovement()
        && (!hadBatch || this.mouseFlushTimer === null)
      ) {
        afterPointerMovement();
      }
    };

    const onPointerMove = (event: PointerEvent) => {
      try {
        if (document?.body?.dataset?.sidebarOpen === "1") return;
      } catch {}
      if (this.dependencies.isInputBlocked()) return;
      if (event.pointerType && event.pointerType !== "mouse") {
        return;
      }

      if (isPointerLockActive()) {
        if (hasPointerRawUpdate && event.type === "pointerrawupdate") {
          if (event.movementX === 0 && event.movementY === 0) {
            const clientMoved =
              event.clientX !== lastPointerClientX || event.clientY !== lastPointerClientY;
            lastPointerClientX = event.clientX;
            lastPointerClientY = event.clientY;
            if (clientMoved && ++pointerRawStuckCount >= 8) {
              this.dependencies.log("pointerrawupdate stuck; switching to immediate mouse flush");
              this.mouseFlushIntervalMs = 0;
              pointerRawStuckCount = 0;
            }
          } else {
            pointerRawStuckCount = 0;
          }
        }

        const samples = hasCoalescedEvents ? event.getCoalescedEvents() : [];
        if (samples.length > 0) {
          processRelativePointerSamples(samples);
          return;
        }
        processRelativePointerSamples([event]);
      } else if (mouseInStreamView) {
        const rect = pointerLockTarget.getBoundingClientRect();
        const absX = event.clientX - rect.left;
        const absY = event.clientY - rect.top;
        lastAbsX = absX;
        lastAbsY = absY;
        if (escapePointerFallbackActive) {
          queueUnlockedAbsolutePointer(event);
        }
      }
    };

    const onMouseMove = (event: MouseEvent) => {
      try {
        if (document?.body?.dataset?.sidebarOpen === "1") return;
      } catch {}
      if (this.dependencies.isInputBlocked()) return;
      if (isPointerLockActive()) {
        processRelativePointerSamples([event]);
      } else if (mouseInStreamView) {
        const rect = pointerLockTarget.getBoundingClientRect();
        const absX = event.clientX - rect.left;
        const absY = event.clientY - rect.top;
        lastAbsX = absX;
        lastAbsY = absY;
        if (escapePointerFallbackActive) {
          queueUnlockedAbsolutePointer(event);
        }
      }
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (this.dependencies.isInputBlocked()) return;
      if (!this.dependencies.isInputReady()) {
        return;
      }

      this.syncLockKeysState(event);

      const isEscapeEvent =
        event.key === "Escape"
        || event.key === "Esc"
        || event.code === "Escape"
        || event.keyCode === 27;
      const mapped = mapKeyboardEvent(event, this.dependencies.getKeyboardLayout()) ?? (isEscapeEvent ? codeMap.Escape : null);

      // Keep browser from handling held keys (for example Tab focus traversal)
      // while streaming input is active.
      if (event.repeat) {
        if (isPointerLockActive() || mapped) {
          event.preventDefault();
        }
        return;
      }

      if (isPointerLockActive()) {
        event.preventDefault();
      }

      if (!mapped) {
        return;
      }

      if (this.pressedKeys.has(mapped.vk)) {
        event.preventDefault();
        return;
      }

      event.preventDefault();
      this.pressedKeys.add(mapped.vk);

      const eventTimestampUs = timestampUs(event.timeStamp);

      const payload = this.dependencies.inputEncoder.encodeKeyDown({
        keycode: mapped.vk,
        scancode: mapped.scancode,
        modifiers: modifierFlags(event),
        timestampUs: eventTimestampUs,
      });
      this.dependencies.sendReliableSingleInput(payload);
    };

    const onKeyUp = (event: KeyboardEvent) => {
      if (this.dependencies.isInputBlocked()) return;
      if (!this.dependencies.isInputReady()) {
        return;
      }

      this.syncLockKeysState(event);

      const isEscapeEvent =
        event.key === "Escape"
        || event.key === "Esc"
        || event.code === "Escape"
        || event.keyCode === 27;
      const mapped = mapKeyboardEvent(event, this.dependencies.getKeyboardLayout()) ?? (isEscapeEvent ? codeMap.Escape : null);
      if (!mapped) {
        return;
      }

      event.preventDefault();
      const eventTimestampUs = timestampUs(event.timeStamp);
      const modifiers = modifierFlags(event);

      if (!this.pressedKeys.has(mapped.vk)) {
        return;
      }

      event.preventDefault();
      this.pressedKeys.delete(mapped.vk);
      this.dependencies.sendReliableSingleInput(this.dependencies.inputEncoder.encodeKeyUp({
        keycode: mapped.vk,
        scancode: mapped.scancode,
        modifiers,
        timestampUs: eventTimestampUs,
      }));
    };

    const onMouseDown = (event: MouseEvent) => {
      if (this.dependencies.isInputBlocked()) return;
      if (!this.dependencies.isInputReady()) {
        return;
      }
      if (!canForwardStreamPointerInput(
        isPointerLockActive(),
        escapePointerFallbackActive,
        mouseInStreamView,
      )) {
        return;
      }
      event.preventDefault();
      if (escapePointerFallbackActive && !isPointerLockActive()) {
        queueUnlockedAbsolutePointer(event, false);
      }
      flushMouse(true);
      const payload = this.dependencies.inputEncoder.encodeMouseButtonDown({
        button: toMouseButton(event.button),
        timestampUs: timestampUs(event.timeStamp),
      });
      // Official GFN client sends all mouse events on reliable channel (input_channel_v1)
      this.dependencies.sendReliableSingleInput(payload);
    };

    const onMouseUp = (event: MouseEvent) => {
      if (this.dependencies.isInputBlocked()) return;
      if (!this.dependencies.isInputReady()) {
        return;
      }
      if (!canForwardStreamPointerInput(
        isPointerLockActive(),
        escapePointerFallbackActive,
        mouseInStreamView,
      )) {
        return;
      }
      event.preventDefault();
      if (escapePointerFallbackActive && !isPointerLockActive()) {
        queueUnlockedAbsolutePointer(event, false);
      }
      flushMouse(true);
      const payload = this.dependencies.inputEncoder.encodeMouseButtonUp({
        button: toMouseButton(event.button),
        timestampUs: timestampUs(event.timeStamp),
      });
      // Official GFN client sends all mouse events on reliable channel (input_channel_v1)
      this.dependencies.sendReliableSingleInput(payload);
    };

    const onWheel = (event: WheelEvent) => {
      if (this.dependencies.isInputBlocked()) return;
      if (!this.dependencies.isInputReady()) {
        return;
      }
      if (!canForwardStreamPointerInput(
        isPointerLockActive(),
        escapePointerFallbackActive,
        mouseInStreamView,
      )) {
        return;
      }
      event.preventDefault();
      if (escapePointerFallbackActive && !isPointerLockActive()) {
        queueUnlockedAbsolutePointer(event, false);
      }
      flushMouse(true);
      // Official GFN client sends negated raw deltaY as int16 (no quantization to ±120).
      // Clamp to int16 range since browser deltaY can exceed it with fast scrolling.
      const delta = Math.max(-32768, Math.min(32767, Math.round(-event.deltaY)));
      const payload = this.dependencies.inputEncoder.encodeMouseWheel({
        delta,
        timestampUs: timestampUs(event.timeStamp),
      });
      this.dependencies.sendReliableSingleInput(payload);
    };

    const onClick = () => {
      focusPointerLockTarget();
      void this.requestPointerLockWithOptionalFullscreen(pointerLockTarget, this.dependencies.shouldAutoFullscreen()).catch(
        (err: DOMException) => {
          this.dependencies.log(`Pointer lock request failed: ${err.name}: ${err.message}`);
        },
      );
      videoElement.focus();
    };

    const schedulePointerLockRetention = (reason: string): void => {
      if (this.pointerLockRelockTimer !== null) {
        return;
      }

      this.pointerLockRelockTimer = window.setTimeout(() => {
        this.pointerLockRelockTimer = null;

        if (!this.dependencies.isInputReady() || !this.shouldSendSyntheticEscapeOnPointerLockLoss() || isPointerLockActive()) {
          return;
        }

        const target = this.pointerLockTarget;
        if (!target) {
          return;
        }

        void this.requestPointerLockWithOptionalFullscreen(target, false)
          .then(() => {
            this.dependencies.log(`Pointer lock restored after ${reason}`);
          })
          .catch((error: unknown) => {
            this.dependencies.log(`Pointer lock restore failed after ${reason}: ${String(error)}`);
          });
      }, 75);
    };

    // Store lock target for pointer lock re-acquisition
    this.pointerLockTarget = pointerLockTarget;

    // Handle pointer lock changes — send synthetic Escape when lock is lost by browser
    // (matches official GFN client's "pointerLockEscape" feature)
    const onPointerLockChange = () => {
      const pointerLockIsActive = isPointerLockActive();
      if (pointerLockIsActive) {
        pointerLockWasActive = true;
        escapePointerFallbackActive = false;
        this.cursorOverlay?.setPointerLocked(true);
        // Pointer lock gained — cancel any pending synthetic Escape.
        // Reset absolute position tracking since we switch to relative movement.
        lastAbsX = null;
        lastAbsY = null;
        if (this.pointerLockEscapeTimer !== null) {
          window.clearTimeout(this.pointerLockEscapeTimer);
          this.pointerLockEscapeTimer = null;
        }
        if (this.pointerLockRelockTimer !== null) {
          window.clearTimeout(this.pointerLockRelockTimer);
          this.pointerLockRelockTimer = null;
        }
        this.clearSyntheticEscapeSuppression();
        // Try to acquire keyboard lock for low-level key capture (best-effort).
        try {
          this.requestEscapeKeyboardLock();
        } catch {}

        // Notify main process that pointer lock is active so native-level
        // interception (before-input-event) can act accordingly.
        try {
          (window as any).openNow?.notifyPointerLockChange?.(true);
        } catch {}
        return;
      }

      if (!didStreamPointerLockExit(pointerLockWasActive, pointerLockIsActive)) {
        return;
      }
      pointerLockWasActive = false;

      const suppressEscapeFullscreenGrace = this.suppressNextSyntheticEscape;
      this.cursorOverlay?.setPointerLocked(false);

      // Pointer lock was lost — reset mirror state so tracking resumes from the
      // current cursor position rather than from a stale last-known position.
      lastAbsX = null;
      lastAbsY = null;

      try {
        (window as any).openNow?.notifyPointerLockChange?.(false, suppressEscapeFullscreenGrace);
      } catch {}

      // Pointer lock was lost
      if (!this.dependencies.isInputReady()) return;

      if (this.consumeSyntheticEscapeSuppression()) {
        escapePointerFallbackActive = false;
        this.releasePressedKeys("pointer lock intentionally released");
        return;
      }

      if (!this.shouldSendSyntheticEscapeOnPointerLockLoss()) {
        escapePointerFallbackActive = false;
        this.releasePressedKeys("pointer lock lost while unfocused");
        return;
      }

      escapePointerFallbackActive = true;

      // VK 0x1B = 27 = Escape
      const escapeWasPressed = this.pressedKeys.has(0x1B);

      if (escapeWasPressed) {
        // Escape was already tracked as pressed — the normal keyup handler will fire
        // and send Escape keyup to the server. No synthetic needed, but Chromium
        // still released pointer lock, so restore it after keyup has a chance to run.
        schedulePointerLockRetention("tracked Escape");
        return;
      }

      // Escape was NOT tracked as pressed — browser intercepted it before our keydown fired.
      // Send synthetic Escape keydown+keyup after 50ms (matches official GFN client).
      // Also re-acquire pointer lock so the user stays in the game.
      this.pointerLockEscapeTimer = window.setTimeout(() => {
        this.pointerLockEscapeTimer = null;

        if (!this.dependencies.isInputReady()) return;

        if (!this.shouldSendSyntheticEscapeOnPointerLockLoss()) {
          this.releasePressedKeys("focus changed before synthetic Escape");
          return;
        }

        // Release all currently held keys first (matching official client's MS() function)
        this.releasePressedKeys("pointer lock lost before synthetic Escape");

        // Send synthetic Escape keydown + keyup
        this.dependencies.log("Sending synthetic Escape (pointer lock lost by browser)");
        const escDown = this.dependencies.inputEncoder.encodeKeyDown({
          keycode: 0x1B,
          scancode: codeMap.Escape.scancode,
          modifiers: 0,
          timestampUs: timestampUs(),
        });
        this.dependencies.sendReliableSingleInput(escDown);

        const escUp = this.dependencies.inputEncoder.encodeKeyUp({
          keycode: 0x1B,
          scancode: codeMap.Escape.scancode,
          modifiers: 0,
          timestampUs: timestampUs(),
        });
        this.dependencies.sendReliableSingleInput(escUp);

        schedulePointerLockRetention("synthetic Escape");
      }, 50);
    };

    const onWindowBlur = () => {
      // Don't release keys during microphone permission request
      // as getUserMedia() may cause brief window focus loss
      if (this.dependencies.getMicState() === "permission_pending") {
        this.dependencies.log("Window blur during mic permission - keeping keys pressed");
        return;
      }
      mouseInStreamView = false;
      escapePointerFallbackActive = false;
      lastAbsX = null;
      lastAbsY = null;
      this.releasePressedKeys("window blur");
      // Pause forwarding while window is not focused (host overlay pause is separate).
      // In native mode the renderer sink can be a separate no-activate window,
      // so a focus transition is not enough reason to stop controller polling.
      if (!this.dependencies.isNativeInputActive()) {
        this.dependencies.setWindowInputPaused(true);
      }
    };

    const onVisibilityChange = () => {
      if (document.visibilityState !== "visible") {
        escapePointerFallbackActive = false;
        this.releasePressedKeys(`visibility ${document.visibilityState}`);
        this.dependencies.setWindowInputPaused(true);
        return;
      }

      this.dependencies.setWindowInputPaused(false);
    };

    const onWindowFocus = () => {
      this.dependencies.setWindowInputPaused(false);
      mouseInStreamView = true;
      lastAbsX = null;
      lastAbsY = null;
      focusPointerLockTarget();
      void this.dependencies.refreshClipboardAvailability();
      // Auto-lock: acquire pointer lock when the user switches back to the app.
      tryAutoLock();
    };

    // Release any prior Keyboard API lock when leaving fullscreen (e.g. other UI may have locked keys).
    const onFullscreenChange = () => {
      if (document.fullscreenElement) {
        this.requestEscapeKeyboardLock();
        return;
      }
      const nav = navigator as any;
      if (nav.keyboard?.unlock) {
        try {
          nav.keyboard.unlock();
          this.keyboardLockState = "unknown";
        } catch {
          /* no-op */
        }
      }
    };

    // Add gamepad event listeners
    window.addEventListener("gamepadconnected", this.dependencies.onGamepadConnected);
    window.addEventListener("gamepaddisconnected", this.dependencies.onGamepadDisconnected);

    document.addEventListener("keydown", onKeyDown, true);
    document.addEventListener("keyup", onKeyUp, true);
    if (pointerMoveEventName) {
      document.addEventListener(pointerMoveEventName, onPointerMove as EventListener);
    } else {
      window.addEventListener("mousemove", onMouseMove);
    }
    // Use document capture for buttons/wheel in native internal mode so clicks
    // still reach us even if the native child HWND is topmost for a frame.
    const buttonTarget: HTMLElement | Document = this.dependencies.isNativeElectronInputBridge()
      ? document
      : pointerLockTarget;
    const buttonCapture = this.dependencies.isNativeElectronInputBridge();
    buttonTarget.addEventListener("mousedown", onMouseDown as EventListener, buttonCapture);
    buttonTarget.addEventListener("mouseup", onMouseUp as EventListener, buttonCapture);
    buttonTarget.addEventListener("wheel", onWheel as EventListener, {
      passive: false,
      capture: buttonCapture,
    } as AddEventListenerOptions);
    pointerLockTarget.addEventListener("mouseenter", onPointerLockTargetMouseEnter);
    pointerLockTarget.addEventListener("mouseleave", onPointerLockTargetMouseLeave);
    // Detect when the mouse enters the application window (from outside the
    // browsing context) and trigger auto pointer lock. We listen to
    // `pointerover` when PointerEvents are available and fall back to
    // `mouseover` for older environments. If `relatedTarget` is null or not
    // part of this document, the pointer came from outside the window. Only
    // attempt auto-lock when the pointer is actually over the stream viewport
    // (pointerLockTarget) to avoid accidental locks when the cursor enters
    // over chrome/UI areas.
    const onDocumentPointerEnterWindow = (ev: PointerEvent | MouseEvent) => {
      // Only care about physical mouse pointers
      if (typeof PointerEvent !== "undefined" && ev instanceof PointerEvent) {
        if (ev.pointerType && ev.pointerType !== "mouse") return;
      }

      const related = (ev as any).relatedTarget as Node | null | undefined;
      if (related && document.contains(related)) {
        // relatedTarget is still within this document — this is an intra-document
        // move, not an entry from outside the window.
        return;
      }

      // Only trigger auto-lock if the pointer is actually over the stream
      // viewport (pointerLockTarget). This prevents accidental locks when the
      // cursor enters the window over chrome/UI areas.
      const rect = pointerLockTarget.getBoundingClientRect();
      const clientX = (ev as MouseEvent).clientX;
      const clientY = (ev as MouseEvent).clientY;
      if (!Number.isFinite(clientX) || !Number.isFinite(clientY)) {
        return;
      }

      if (clientX < rect.left || clientX > rect.right || clientY < rect.top || clientY > rect.bottom) {
        return;
      }

      // Treat this as entering the stream/window area for auto-lock purposes
      mouseInStreamView = true;
      // Save entry absolute coords so tryAutoLock can align the server cursor
      // before requesting pointer lock.
      pendingEntryAbsX = clientX - rect.left;
      pendingEntryAbsY = clientY - rect.top;
      lastAbsX = null;
      lastAbsY = null;
      tryAutoLock();
    };

    // Fallback: some environments may not produce pointerover relatedTarget=null
    // when entering the native window. Listen for the first mousemove while we
    // believe the pointer is outside the window and treat that as an entry.
    const onFirstMouseMoveIntoWindow = (ev: MouseEvent | PointerEvent) => {
      if (mouseInStreamView) return;
      if (typeof PointerEvent !== "undefined" && ev instanceof PointerEvent) {
        if (ev.pointerType && ev.pointerType !== "mouse") return;
      }

      // Only consider it an entry if the cursor is over the stream viewport
      const rect = pointerLockTarget.getBoundingClientRect();
      const clientX = (ev as MouseEvent).clientX;
      const clientY = (ev as MouseEvent).clientY;
      if (!Number.isFinite(clientX) || !Number.isFinite(clientY)) return;
      if (clientX < rect.left || clientX > rect.right || clientY < rect.top || clientY > rect.bottom) return;

      mouseInStreamView = true;
      lastAbsX = null;
      lastAbsY = null;
      tryAutoLock();
      // remove this listener after first use
      document.removeEventListener("mousemove", onFirstMouseMoveIntoWindow as EventListener, true);
      if (typeof PointerEvent !== "undefined") {
        document.removeEventListener("pointermove", onFirstMouseMoveIntoWindow as EventListener, true);
      }
    };
    videoElement.addEventListener("click", onClick);
    if (typeof PointerEvent !== "undefined") {
      document.addEventListener("pointerover", onDocumentPointerEnterWindow, true);
      document.addEventListener("pointermove", onFirstMouseMoveIntoWindow as EventListener, true);
    } else {
      document.addEventListener("mouseover", onDocumentPointerEnterWindow, true);
      document.addEventListener("mousemove", onFirstMouseMoveIntoWindow as EventListener, true);
    }
    focusPointerLockTarget();
    document.addEventListener("pointerlockchange", onPointerLockChange);
    document.addEventListener("fullscreenchange", onFullscreenChange);
    window.addEventListener("blur", onWindowBlur);
    document.addEventListener("visibilitychange", onVisibilityChange);
    window.addEventListener("focus", onWindowFocus);

    this.inputCleanup.push(() => window.removeEventListener("gamepadconnected", this.dependencies.onGamepadConnected));
    this.inputCleanup.push(() => window.removeEventListener("gamepaddisconnected", this.dependencies.onGamepadDisconnected));
    this.inputCleanup.push(() => document.removeEventListener("keydown", onKeyDown, true));
    this.inputCleanup.push(() => document.removeEventListener("keyup", onKeyUp, true));
    if (pointerMoveEventName) {
      this.inputCleanup.push(() => document.removeEventListener(pointerMoveEventName, onPointerMove as EventListener));
    } else {
      this.inputCleanup.push(() => window.removeEventListener("mousemove", onMouseMove));
    }
    this.inputCleanup.push(() => {
      buttonTarget.removeEventListener("mousedown", onMouseDown as EventListener, buttonCapture);
      buttonTarget.removeEventListener("mouseup", onMouseUp as EventListener, buttonCapture);
      buttonTarget.removeEventListener("wheel", onWheel as EventListener, {
        capture: buttonCapture,
      } as EventListenerOptions);
    });
    this.inputCleanup.push(() => pointerLockTarget.removeEventListener("mouseenter", onPointerLockTargetMouseEnter));
    this.inputCleanup.push(() => pointerLockTarget.removeEventListener("mouseleave", onPointerLockTargetMouseLeave));
    if (typeof PointerEvent !== "undefined") {
      this.inputCleanup.push(() => document.removeEventListener("pointerover", onDocumentPointerEnterWindow, true));
      this.inputCleanup.push(() => document.removeEventListener("pointermove", onFirstMouseMoveIntoWindow as EventListener, true));
    } else {
      this.inputCleanup.push(() => document.removeEventListener("mouseover", onDocumentPointerEnterWindow, true));
      this.inputCleanup.push(() => document.removeEventListener("mousemove", onFirstMouseMoveIntoWindow as EventListener, true));
    }
    this.inputCleanup.push(() => videoElement.removeEventListener("click", onClick));
    this.inputCleanup.push(() => {
      if (originalPointerLockTargetTabIndex === null) {
        pointerLockTarget.removeAttribute("tabindex");
      } else {
        pointerLockTarget.setAttribute("tabindex", originalPointerLockTargetTabIndex);
      }
    });
    this.inputCleanup.push(() => document.removeEventListener("pointerlockchange", onPointerLockChange));
    this.inputCleanup.push(() => document.removeEventListener("fullscreenchange", onFullscreenChange));
    this.inputCleanup.push(() => window.removeEventListener("blur", onWindowBlur));
    this.inputCleanup.push(() => document.removeEventListener("visibilitychange", onVisibilityChange));
    this.inputCleanup.push(() => window.removeEventListener("focus", onWindowFocus));
    this.inputCleanup.push(() => {
      if (this.pointerLockEscapeTimer !== null) {
        window.clearTimeout(this.pointerLockEscapeTimer);
        this.pointerLockEscapeTimer = null;
      }
      if (this.pointerLockRelockTimer !== null) {
        window.clearTimeout(this.pointerLockRelockTimer);
        this.pointerLockRelockTimer = null;
      }
      this.clearSyntheticEscapeSuppression();
      this.releasePressedKeys("input cleanup");
      this.pendingMouseDxFloat = 0;
      this.pendingMouseDyFloat = 0;
      this.pendingMouseAbs = null;
      this.pendingMouseTimestampUs = null;
      this.mouseDeltaFilter.reset();
      this.pointerLockTarget = null;
      // Unlock keyboard on cleanup
      const nav = navigator as any;
      if (nav.keyboard?.unlock) {
        nav.keyboard.unlock();
      }
    });
  }

  /**
   * Query browser for supported video codecs via RTCRtpReceiver.getCapabilities.
   * Returns normalized names like "H264", "H265", "AV1", "VP9", "VP8".
   */
}
