import { useEffect, useRef } from "react";

import {
  createControllerEdgeState,
  stepControllerFrame,
  type ControllerFrame,
  type ControllerStepOptions,
} from "../lib/controllerInputState";
import { readControllerGamepadButtons } from "../utils/controllerGamepad";

export interface UseControllerNavigationOptions extends ControllerStepOptions {
  /** Typically `controllerMode && surfaceActive`. */
  enabled: boolean;
  onFrame: (frame: ControllerFrame) => void;
  /** Start/stop the loop on gamepad connect/disconnect. Defaults to true. */
  listenForConnection?: boolean;
}

export interface ControllerNavigation {
  resetEdgeState: () => void;
}

function readConnectedPadButtons(): number {
  const pad = navigator.getGamepads?.().find((gamepad): gamepad is Gamepad => Boolean(gamepad));
  return readControllerGamepadButtons(pad);
}

/**
 * Single owner of the gamepad polling loop for console surfaces.
 *
 * `onFrame` is mirrored into a ref on every render, so the effect deps stay
 * limited to the polling options — callers can pass inline closures without
 * restarting the loop each commit.
 */
export function useControllerNavigation({
  enabled,
  onFrame,
  listenForConnection = true,
  repeatMs,
  repeatMask,
  holdMask,
  holdMs,
}: UseControllerNavigationOptions): ControllerNavigation {
  const handlerRef = useRef(onFrame);
  handlerRef.current = onFrame;

  const edgeStateRef = useRef(createControllerEdgeState());
  const frameRef = useRef<number | null>(null);

  useEffect(() => {
    if (!enabled) {
      edgeStateRef.current = createControllerEdgeState();
      return undefined;
    }

    const stepOptions: ControllerStepOptions = { repeatMs, repeatMask, holdMask, holdMs };

    const handleFrame = (): void => {
      const frame = stepControllerFrame(
        edgeStateRef.current,
        readConnectedPadButtons(),
        performance.now(),
        stepOptions,
      );
      edgeStateRef.current = frame.state;
      handlerRef.current(frame);
      frameRef.current = window.requestAnimationFrame(handleFrame);
    };

    const start = (): void => {
      if (frameRef.current !== null) return;
      // Seed from the live pad so buttons already down when the loop starts do
      // not register as a fresh press.
      edgeStateRef.current = {
        ...createControllerEdgeState(),
        previousButtons: readConnectedPadButtons(),
        lastMoveAtMs: performance.now(),
      };
      frameRef.current = window.requestAnimationFrame(handleFrame);
    };

    const stop = (): void => {
      if (frameRef.current !== null) {
        window.cancelAnimationFrame(frameRef.current);
        frameRef.current = null;
      }
      edgeStateRef.current = createControllerEdgeState();
    };

    const handleDisconnect = (): void => {
      const hasConnectedPad = navigator.getGamepads?.().some(Boolean) ?? false;
      if (!hasConnectedPad) stop();
    };

    if (listenForConnection) {
      window.addEventListener("gamepadconnected", start);
      window.addEventListener("gamepaddisconnected", handleDisconnect);
    }
    start();

    return () => {
      if (listenForConnection) {
        window.removeEventListener("gamepadconnected", start);
        window.removeEventListener("gamepaddisconnected", handleDisconnect);
      }
      stop();
    };
  }, [enabled, holdMask, holdMs, listenForConnection, repeatMask, repeatMs]);

  const navigationRef = useRef<ControllerNavigation>({
    resetEdgeState: () => {
      edgeStateRef.current = createControllerEdgeState();
    },
  });
  return navigationRef.current;
}

/**
 * Window keydown listener whose handler is ref-mirrored, so the listener is
 * bound once per enabled-state change instead of on every render.
 */
export function useControllerKeyDown(enabled: boolean, onKeyDown: (event: KeyboardEvent) => void): void {
  const handlerRef = useRef(onKeyDown);
  handlerRef.current = onKeyDown;

  useEffect(() => {
    if (!enabled) return undefined;
    const handle = (event: KeyboardEvent): void => handlerRef.current(event);
    window.addEventListener("keydown", handle);
    return () => window.removeEventListener("keydown", handle);
  }, [enabled]);
}
