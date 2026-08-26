export const POINTER_LOCK_ESCAPE_FULLSCREEN_GRACE_MS = 1000;
/** Match native presenter's Escape-hold timing. */
export const ESCAPE_HOLD_TO_EXIT_FULLSCREEN_MS = 1500;

export interface EscapeKeyInput {
  type?: string;
  key?: string;
  code?: string;
  keyCode?: number;
  isAutoRepeat?: boolean;
}

export interface EscapeFullscreenGuardState {
  allowEscapeToExitFullscreen: boolean;
  streamInputActive: boolean;
  pointerLockActive: boolean;
  rendererControlledFullscreen: boolean;
  windowFullscreen: boolean;
  pointerLockEscapeCaptureUntilMs: number;
  nowMs: number;
}

export type EscapeHoldCaptureAction =
  | "ignore"
  | "arm-hold"
  | "hold-repeat"
  | "tap"
  | "hold-consumed-keyup";

export interface EscapeHoldCaptureState {
  keyDownCaptured: boolean;
  holdFired: boolean;
}

export function isEscapeKeyInput(input: EscapeKeyInput): boolean {
  return (
    input.key === "Escape" ||
    input.key === "Esc" ||
    input.code === "Escape" ||
    input.keyCode === 27
  );
}

export function isEscapeKeyDownInput(input: EscapeKeyInput): boolean {
  return input.type === "keyDown" && isEscapeKeyInput(input);
}

export function isEscapeKeyUpInput(input: EscapeKeyInput): boolean {
  return input.type === "keyUp" && isEscapeKeyInput(input);
}

export function shouldCaptureEscapeFullscreenInput(
  input: EscapeKeyInput,
  state: EscapeFullscreenGuardState,
): boolean {
  if (!isEscapeKeyDownInput(input) || state.allowEscapeToExitFullscreen) {
    return false;
  }

  if (state.pointerLockActive) {
    return true;
  }

  // Electron's fullscreen state can lag the renderer IPC request. Protect both
  // signals, but only while a stream input route is actually active.
  if (
    state.streamInputActive
    && (state.windowFullscreen || state.rendererControlledFullscreen)
  ) {
    return true;
  }

  return state.windowFullscreen && state.nowMs <= state.pointerLockEscapeCaptureUntilMs;
}

/**
 * Electron Internal/web path: Escape tap → game, hold → exit fullscreen.
 * Mirrors native Internal RawInput hold timing without sending a tap after hold.
 */
export function resolveEscapeHoldCaptureAction(
  input: EscapeKeyInput,
  guardState: EscapeFullscreenGuardState,
  holdState: EscapeHoldCaptureState,
): { action: EscapeHoldCaptureAction; nextHoldState: EscapeHoldCaptureState } {
  if (guardState.allowEscapeToExitFullscreen || !isEscapeKeyInput(input)) {
    return {
      action: "ignore",
      nextHoldState: { keyDownCaptured: false, holdFired: false },
    };
  }

  if (isEscapeKeyDownInput(input)) {
    if (!shouldCaptureEscapeFullscreenInput(input, guardState) && !holdState.keyDownCaptured) {
      return { action: "ignore", nextHoldState: holdState };
    }

    if (holdState.keyDownCaptured || input.isAutoRepeat) {
      return {
        action: "hold-repeat",
        nextHoldState: { keyDownCaptured: true, holdFired: holdState.holdFired },
      };
    }

    return {
      action: "arm-hold",
      nextHoldState: { keyDownCaptured: true, holdFired: false },
    };
  }

  if (isEscapeKeyUpInput(input) && holdState.keyDownCaptured) {
    if (holdState.holdFired) {
      return {
        action: "hold-consumed-keyup",
        nextHoldState: { keyDownCaptured: false, holdFired: false },
      };
    }
    return {
      action: "tap",
      nextHoldState: { keyDownCaptured: false, holdFired: false },
    };
  }

  return { action: "ignore", nextHoldState: holdState };
}

export function markEscapeHoldFired(
  holdState: EscapeHoldCaptureState,
): EscapeHoldCaptureState {
  if (!holdState.keyDownCaptured) {
    return holdState;
  }
  return { keyDownCaptured: true, holdFired: true };
}

export function nextPointerLockEscapeCaptureUntilMs(
  active: boolean,
  suppressEscapeFullscreenGrace: boolean,
  nowMs: number,
): number {
  if (active || suppressEscapeFullscreenGrace) {
    return 0;
  }

  return nowMs + POINTER_LOCK_ESCAPE_FULLSCREEN_GRACE_MS;
}
