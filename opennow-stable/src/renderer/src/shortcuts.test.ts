import test from "node:test";
import assert from "node:assert/strict";

import { normalizeShortcut, shortcutFromKeyboardEvent } from "./shortcuts";

test("shortcut normalization accepts native-supported named keys", () => {
  const parsed = normalizeShortcut("Ctrl+ContextMenu");

  assert.equal(parsed.valid, true);
  assert.equal(parsed.canonical, "Ctrl+CONTEXTMENU");
});

test("shortcut normalization rejects unsupported named key tokens", () => {
  const parsed = normalizeShortcut("Ctrl+BrowserBack");

  assert.equal(parsed.valid, false);
});

test("shortcut capture ignores unsupported keyboard codes", () => {
  const captured = shortcutFromKeyboardEvent({
    repeat: false,
    code: "BrowserBack",
    key: "BrowserBack",
    ctrlKey: true,
    altKey: false,
    shiftKey: false,
    metaKey: false,
  } as KeyboardEvent);

  assert.equal(captured, null);
});
