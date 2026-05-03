import { useEffect } from "react";
import type { MutableRefObject } from "react";
import type { ControllerLibraryEventHandlers } from "./useControllerLibraryEvents";

type ControllerEventHandlersRef = MutableRefObject<ControllerLibraryEventHandlers>;

export function useControllerWindowBindings(controllerEventHandlersRef: ControllerEventHandlersRef): void {
  useEffect(() => {
    const directionListener = (event: Event) => controllerEventHandlersRef.current.onDirection(event);
    const shoulderListener = (event: Event) => controllerEventHandlersRef.current.onShoulder(event);
    const activateListener = () => controllerEventHandlersRef.current.onActivate();
    const secondaryActivateListener = () => controllerEventHandlersRef.current.onSecondaryActivate();
    const tertiaryActivateListener = () => controllerEventHandlersRef.current.onTertiaryActivate();
    const cancelListener = (event: Event) => controllerEventHandlersRef.current.onCancel(event);
    const keyboardListener = (event: KeyboardEvent) => controllerEventHandlersRef.current.onKeyboard(event);

    window.addEventListener("opennow:controller-direction", directionListener);
    window.addEventListener("opennow:controller-shoulder", shoulderListener);
    window.addEventListener("opennow:controller-activate", activateListener);
    window.addEventListener("opennow:controller-secondary-activate", secondaryActivateListener);
    window.addEventListener("opennow:controller-tertiary-activate", tertiaryActivateListener);
    window.addEventListener("opennow:controller-cancel", cancelListener);
    window.addEventListener("keydown", keyboardListener);
    return () => {
      window.removeEventListener("opennow:controller-direction", directionListener);
      window.removeEventListener("opennow:controller-shoulder", shoulderListener);
      window.removeEventListener("opennow:controller-activate", activateListener);
      window.removeEventListener("opennow:controller-secondary-activate", secondaryActivateListener);
      window.removeEventListener("opennow:controller-tertiary-activate", tertiaryActivateListener);
      window.removeEventListener("opennow:controller-cancel", cancelListener);
      window.removeEventListener("keydown", keyboardListener);
    };
  }, [controllerEventHandlersRef]);
}
