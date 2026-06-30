export const POINTER_LOCK_ESCAPE_FULLSCREEN_GRACE_MS = 1000;

export interface EscapeKeyInput {
  type?: string;
  key?: string;
  code?: string;
  keyCode?: number;
}

export interface EscapeFullscreenGuardState {
  allowEscapeToExitFullscreen: boolean;
  pointerLockActive: boolean;
  windowFullscreen: boolean;
  pointerLockEscapeCaptureUntilMs: number;
  nowMs: number;
}

export function isEscapeKeyDownInput(input: EscapeKeyInput): boolean {
  return input.type === "keyDown" && (
    input.key === "Escape" ||
    input.key === "Esc" ||
    input.code === "Escape" ||
    input.keyCode === 27
  );
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

  return state.windowFullscreen && state.nowMs <= state.pointerLockEscapeCaptureUntilMs;
}
