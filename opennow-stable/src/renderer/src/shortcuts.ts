export interface ParsedShortcut {
  key: string;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  meta: boolean;
  valid: boolean;
  canonical: string;
}

function normalizeKeyToken(token: string): string | null {
  const upper = token.toUpperCase();
  const alias: Record<string, string> = {
    ESC: "ESCAPE",
    RETURN: "ENTER",
    DEL: "DELETE",
    INS: "INSERT",
    PGUP: "PAGEUP",
    PGDN: "PAGEDOWN",
    SPACEBAR: "SPACE",
    " ": "SPACE",
  };

  if (alias[upper]) {
    return alias[upper];
  }
  if (upper.length === 1) {
    return upper;
  }
  if (/^F\d{1,2}$/.test(upper)) {
    return upper;
  }
  if (upper.startsWith("ARROW")) {
    return upper;
  }
  if (/^[A-Z0-9_]+$/.test(upper)) {
    return upper;
  }
  return null;
}

function normalizeEventKey(key: string): string {
  const upper = key.toUpperCase();
  const alias: Record<string, string> = {
    ESC: "ESCAPE",
    " ": "SPACE",
  };
  if (alias[upper]) {
    return alias[upper];
  }
  return upper;
}

function normalizeEventCode(code: string): string | null {
  if (!code) return null;
  const upper = code.toUpperCase();

  if (upper.startsWith("KEY") && upper.length === 4) {
    return upper.slice(3);
  }
  if (upper.startsWith("DIGIT") && upper.length === 6) {
    return upper.slice(5);
  }
  if (upper.startsWith("NUMPAD")) {
    return upper;
  }
  if (/^F\d{1,2}$/.test(upper)) {
    return upper;
  }
  if (upper.startsWith("ARROW")) {
    return upper;
  }
  if (upper === "SPACE") {
    return "SPACE";
  }
  if (upper === "ENTER" || upper === "NUMPADENTER") {
    return "ENTER";
  }
  if (/^[A-Z0-9_]+$/.test(upper)) {
    return upper;
  }
  return null;
}

function isKeyMatch(event: KeyboardEvent, shortcutKey: string): boolean {
  const byKey = normalizeEventKey(event.key) === shortcutKey;
  if (byKey) return true;

  const code = normalizeEventCode(event.code);
  if (!code) return false;

  if (code === shortcutKey) return true;
  if (shortcutKey.length === 1 && (code === `KEY${shortcutKey}` || code === `DIGIT${shortcutKey}`)) {
    return true;
  }
  if (shortcutKey === "ENTER" && code === "NUMPADENTER") {
    return true;
  }

  return false;
}

export function isShortcutMatch(event: KeyboardEvent, shortcut: ParsedShortcut): boolean {
  if (!shortcut.valid) return false;
  if (event.ctrlKey !== shortcut.ctrl) return false;
  if (event.altKey !== shortcut.alt) return false;
  if (event.shiftKey !== shortcut.shift) return false;
  if (event.metaKey !== shortcut.meta) return false;
  return isKeyMatch(event, shortcut.key);
}

export function normalizeShortcut(raw: string): ParsedShortcut {
  const tokens = raw
    .split("+")
    .map((part) => part.trim())
    .filter(Boolean);

  let ctrl = false;
  let alt = false;
  let shift = false;
  let meta = false;
  let keyToken: string | null = null;

  for (const token of tokens) {
    const upper = token.toUpperCase();
    if (upper === "CTRL" || upper === "CONTROL") {
      ctrl = true;
      continue;
    }
    if (upper === "ALT" || upper === "OPTION") {
      alt = true;
      continue;
    }
    if (upper === "SHIFT") {
      shift = true;
      continue;
    }
    if (upper === "META" || upper === "CMD" || upper === "COMMAND") {
      meta = true;
      continue;
    }
    if (keyToken) {
      return {
        key: "",
        ctrl,
        alt,
        shift,
        meta,
        valid: false,
        canonical: raw.trim(),
      };
    }
    keyToken = token;
  }

  if (!keyToken) {
    return {
      key: "",
      ctrl,
      alt,
      shift,
      meta,
      valid: false,
      canonical: raw.trim(),
    };
  }

  const normalizedKey = normalizeKeyToken(keyToken);
  if (!normalizedKey) {
    return {
      key: "",
      ctrl,
      alt,
      shift,
      meta,
      valid: false,
      canonical: raw.trim(),
    };
  }

  const parts: string[] = [];
  if (ctrl) parts.push("Ctrl");
  if (alt) parts.push("Alt");
  if (shift) parts.push("Shift");
  if (meta) parts.push("Meta");
  parts.push(normalizedKey);

  return {
    key: normalizedKey,
    ctrl,
    alt,
    shift,
    meta,
    valid: true,
    canonical: parts.join("+"),
  };
}

export function formatShortcutForDisplay(raw: string, isMac: boolean): string {
  const parsed = normalizeShortcut(raw);
  if (!parsed.valid) {
    return raw;
  }

  const parts: string[] = [];
  if (parsed.ctrl) parts.push("Ctrl");
  if (parsed.alt) parts.push(isMac ? "Option" : "Alt");
  if (parsed.shift) parts.push("Shift");
  if (parsed.meta) parts.push(isMac ? "Cmd" : "Meta");
  parts.push(parsed.key);
  return parts.join("+");
}
