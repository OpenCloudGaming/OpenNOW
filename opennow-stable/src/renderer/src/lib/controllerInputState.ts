import { controllerButton } from "../utils/controllerGamepad";

/** Auto-repeat delay for held directional input, matching the console shell feel. */
export const CONTROLLER_MOVE_REPEAT_MS = 140;
/** How long a button must be held before it counts as a hold gesture rather than a tap. */
export const CONTROLLER_HOLD_MS = 350;

const DEFAULT_REPEAT_MASK =
  controllerButton.up | controllerButton.down | controllerButton.left | controllerButton.right;

export interface ControllerEdgeState {
  previousButtons: number;
  lastMoveAtMs: number;
  holdStartedAtMs: number;
  holdConsumed: boolean;
}

export interface ControllerFrame {
  /** Raw button bitmask for this frame. */
  buttons: number;
  /** Buttons that went down this frame, including re-injected auto-repeat. */
  pressed: number;
  /** Buttons that went up this frame. */
  released: number;
  /** True on the single frame a hold gesture crosses its threshold. */
  holdFired: boolean;
  nowMs: number;
  state: ControllerEdgeState;
}

export interface ControllerStepOptions {
  /** Auto-repeat interval; `null` disables repeat entirely. */
  repeatMs?: number | null;
  /** Buttons eligible for auto-repeat. Defaults to the d-pad. */
  repeatMask?: number;
  /** Buttons that arm the hold gesture. `0` (default) disables it. */
  holdMask?: number;
  holdMs?: number;
}

export function createControllerEdgeState(): ControllerEdgeState {
  return {
    previousButtons: 0,
    lastMoveAtMs: 0,
    holdStartedAtMs: 0,
    holdConsumed: false,
  };
}

/**
 * Pure per-frame edge detection for gamepad input. Owns the semantics every
 * console surface relies on: press/release edges, auto-repeat for held
 * directions, and a latched hold gesture that fires exactly once per press.
 *
 * Returns a new state; the caller stores it for the next frame.
 */
export function stepControllerFrame(
  state: ControllerEdgeState,
  buttons: number,
  nowMs: number,
  options: ControllerStepOptions = {},
): ControllerFrame {
  const {
    repeatMs = CONTROLLER_MOVE_REPEAT_MS,
    repeatMask = DEFAULT_REPEAT_MASK,
    holdMask = 0,
    holdMs = CONTROLLER_HOLD_MS,
  } = options;

  let pressed = buttons & ~state.previousButtons;
  const released = state.previousButtons & ~buttons;

  let lastMoveAtMs = state.lastMoveAtMs;
  if (repeatMs !== null) {
    const activeMoves = buttons & repeatMask;
    const pressedMoves = pressed & repeatMask;
    if (pressedMoves) {
      lastMoveAtMs = nowMs;
    } else if (activeMoves && nowMs - lastMoveAtMs > repeatMs) {
      pressed |= activeMoves;
      lastMoveAtMs = nowMs;
    }
  }

  let holdStartedAtMs = state.holdStartedAtMs;
  let holdConsumed = state.holdConsumed;
  let holdFired = false;
  if (holdMask) {
    if (pressed & holdMask) {
      holdStartedAtMs = nowMs;
      holdConsumed = false;
    }
    if ((buttons & holdMask) && !holdConsumed && nowMs - holdStartedAtMs >= holdMs) {
      holdConsumed = true;
      holdFired = true;
    }
  }

  return {
    buttons,
    pressed,
    released,
    holdFired,
    nowMs,
    state: {
      previousButtons: buttons,
      lastMoveAtMs,
      holdStartedAtMs,
      holdConsumed,
    },
  };
}

/**
 * True when a hold-capable button was released as a tap — i.e. it never crossed
 * the hold threshold. Console surfaces use this to distinguish tap-to-cycle from
 * hold-to-open on the same button.
 */
export function wasReleasedAsTap(frame: ControllerFrame, holdMask: number): boolean {
  return Boolean(frame.released & holdMask) && !frame.state.holdConsumed;
}
