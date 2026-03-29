import { describe, expect, it } from "vitest";

import {
  formatShortcutForDisplay,
  isShortcutMatch,
  normalizeShortcut,
} from "../../src/renderer/src/shortcuts";

function keyboardEvent(overrides: Partial<KeyboardEvent> = {}): KeyboardEvent {
  return {
    key: "",
    code: "",
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    ...overrides,
  } as KeyboardEvent;
}

describe("shortcuts", () => {
  it("normalizes aliases and canonical modifier ordering", () => {
    expect(normalizeShortcut("Esc")).toMatchObject({ valid: true, key: "ESCAPE", canonical: "ESCAPE" });
    expect(normalizeShortcut("Return")).toMatchObject({ valid: true, key: "ENTER", canonical: "ENTER" });
    expect(normalizeShortcut("Del")).toMatchObject({ valid: true, key: "DELETE", canonical: "DELETE" });
    expect(normalizeShortcut("PgUp")).toMatchObject({ valid: true, key: "PAGEUP", canonical: "PAGEUP" });
    expect(normalizeShortcut("PgDn")).toMatchObject({ valid: true, key: "PAGEDOWN", canonical: "PAGEDOWN" });
    expect(normalizeShortcut("Spacebar")).toMatchObject({ valid: true, key: "SPACE", canonical: "SPACE" });
    expect(normalizeShortcut("shift + cmd + alt + ctrl + esc").canonical).toBe("Ctrl+Alt+Shift+Meta+ESCAPE");
  });

  it("rejects invalid shortcut definitions", () => {
    expect(normalizeShortcut("Ctrl+Shift")).toMatchObject({ valid: false, key: "" });
    expect(normalizeShortcut("Ctrl+A+B")).toMatchObject({ valid: false, key: "" });
    expect(normalizeShortcut("Ctrl+?")).toMatchObject({ valid: false, key: "" });
  });

  it("matches shortcuts by key and falls back to code", () => {
    const shortcut = normalizeShortcut("Ctrl+Shift+A");

    expect(isShortcutMatch(keyboardEvent({ key: "a", code: "KeyA", ctrlKey: true, shiftKey: true }), shortcut)).toBe(true);
    expect(isShortcutMatch(keyboardEvent({ key: "Dead", code: "KeyA", ctrlKey: true, shiftKey: true }), shortcut)).toBe(true);
    expect(isShortcutMatch(keyboardEvent({ key: "a", code: "KeyA", ctrlKey: true, shiftKey: false }), shortcut)).toBe(false);
  });

  it("treats numpad enter as enter", () => {
    const shortcut = normalizeShortcut("Enter");
    expect(isShortcutMatch(keyboardEvent({ key: "Enter", code: "NumpadEnter" }), shortcut)).toBe(true);
  });

  it("formats shortcuts differently for mac and non-mac labels", () => {
    expect(formatShortcutForDisplay("Ctrl+Alt+Meta+Spacebar", false)).toBe("Ctrl+Alt+Meta+SPACE");
    expect(formatShortcutForDisplay("Ctrl+Alt+Meta+Spacebar", true)).toBe("Ctrl+Option+Cmd+SPACE");
    expect(formatShortcutForDisplay("Ctrl+?", true)).toBe("Ctrl+?");
  });
});
