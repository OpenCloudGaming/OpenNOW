import type { NativeStreamerShortcutAction } from "@shared/gfn";

const STREAM_SHORTCUT_ACTION_EVENT = "opennow:stream-shortcut-action";

export function dispatchStreamShortcutAction(action: NativeStreamerShortcutAction): void {
  window.dispatchEvent(new CustomEvent<NativeStreamerShortcutAction>(STREAM_SHORTCUT_ACTION_EVENT, {
    detail: action,
  }));
}

export function addStreamShortcutActionListener(
  listener: (action: NativeStreamerShortcutAction) => void,
): () => void {
  const handler: EventListener = (event) => {
    listener((event as CustomEvent<NativeStreamerShortcutAction>).detail);
  };

  window.addEventListener(STREAM_SHORTCUT_ACTION_EVENT, handler);
  return () => {
    window.removeEventListener(STREAM_SHORTCUT_ACTION_EVENT, handler);
  };
}
