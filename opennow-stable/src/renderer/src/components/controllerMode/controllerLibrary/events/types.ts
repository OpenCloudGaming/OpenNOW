import type { Direction } from "../types";

export type ControllerLibraryEventHandlers = {
  onDirection: (event: Event) => void;
  onShoulder: (event: Event) => void;
  onActivate: () => void;
  onSecondaryActivate: () => void;
  onTertiaryActivate: () => void;
  onCancel: (event: Event) => void;
  onKeyboard: (event: KeyboardEvent) => void;
};

// Intentionally broad context typing while extraction stabilizes behavior parity.
export type ControllerLibraryEventContext = Record<string, any>;

export type ApplyDirection = (direction: Direction) => void;
export type CycleTopCategory = (delta: number) => void;
