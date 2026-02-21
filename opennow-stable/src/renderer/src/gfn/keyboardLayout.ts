/**
 * Keyboard layout detection and mapping for cloud gaming.
 *
 * Cloud gaming sends physical scancodes (USB HID) to the remote machine.
 * The remote OS applies its own keyboard layout to interpret them.
 * Therefore, the correct default behavior ("auto") is to always send the
 * physical scancode derived from KeyboardEvent.code — this means the
 * remote OS layout determines what character is produced, which is correct
 * for most users (their remote VM layout matches their physical keyboard).
 *
 * The VK (virtual key) code sent alongside the scancode is used by some
 * games and Windows APIs. In "auto" mode we send the QWERTY-based VK
 * that matches the physical key position (same as current behavior).
 *
 * Override modes ("azerty", "qwertz") remap the VK code so that the
 * remote side receives a VK matching the character the user expects
 * from that physical key position on their layout. This can help when
 * the remote OS layout is QWERTY but the user has a non-QWERTY keyboard
 * and wants character-correct input without changing remote OS settings.
 *
 * Scancodes are NEVER remapped — they are always physical-position-based.
 */

import type { KeyboardLayout } from "@shared/gfn";

export type DetectedLayout = "qwerty" | "azerty" | "qwertz" | "unknown";

interface LayoutDetectionResult {
  detected: DetectedLayout;
  method: "keyboard-api" | "language-heuristic" | "none";
  confidence: "high" | "medium" | "low";
}

let cachedDetection: LayoutDetectionResult | null = null;

/**
 * Detect the OS keyboard layout using the best available method.
 * Result is cached after first successful detection.
 */
export async function detectKeyboardLayout(): Promise<LayoutDetectionResult> {
  if (cachedDetection) return cachedDetection;

  // Method 1: navigator.keyboard.getLayoutMap() (Chromium 69+)
  // This is the most reliable method — it queries the OS for the actual
  // character each physical key produces.
  const nav = navigator as Navigator & {
    keyboard?: { getLayoutMap?: () => Promise<Map<string, string>> };
  };

  if (nav.keyboard?.getLayoutMap) {
    try {
      const layoutMap = await nav.keyboard.getLayoutMap();
      const keyA = layoutMap.get("KeyA"); // QWERTY: 'a', AZERTY: 'q'
      const keyQ = layoutMap.get("KeyQ"); // QWERTY: 'q', AZERTY: 'a'
      const keyW = layoutMap.get("KeyW"); // QWERTY: 'w', AZERTY: 'z'
      const keyZ = layoutMap.get("KeyZ"); // QWERTY: 'z', QWERTZ: 'y'
      const keyY = layoutMap.get("KeyY"); // QWERTY: 'y', QWERTZ: 'z'

      // AZERTY: physical KeyA produces 'q', physical KeyQ produces 'a'
      if (keyA === "q" && keyQ === "a") {
        cachedDetection = { detected: "azerty", method: "keyboard-api", confidence: "high" };
        return cachedDetection;
      }

      // QWERTZ: physical KeyZ produces 'y', physical KeyY produces 'z'
      if (keyZ === "y" && keyY === "z") {
        cachedDetection = { detected: "qwertz", method: "keyboard-api", confidence: "high" };
        return cachedDetection;
      }

      // If KeyA='a' and KeyQ='q' and KeyW='w', it's QWERTY
      if (keyA === "a" && keyQ === "q" && keyW === "w") {
        cachedDetection = { detected: "qwerty", method: "keyboard-api", confidence: "high" };
        return cachedDetection;
      }

      // Layout detected but doesn't match known patterns
      cachedDetection = { detected: "unknown", method: "keyboard-api", confidence: "medium" };
      return cachedDetection;
    } catch {
      // getLayoutMap() failed — fall through to heuristic
    }
  }

  // Method 2: Language-based heuristic (less reliable, best-effort)
  // navigator.language gives the browser UI language which often correlates
  // with keyboard layout, but not always (e.g., French user with QWERTY).
  const lang = (navigator.language || "").toLowerCase();
  const primary = lang.split("-")[0];

  if (primary === "fr") {
    cachedDetection = { detected: "azerty", method: "language-heuristic", confidence: "low" };
    return cachedDetection;
  }
  if (primary === "de" || primary === "cs" || primary === "sk" || primary === "hu") {
    cachedDetection = { detected: "qwertz", method: "language-heuristic", confidence: "low" };
    return cachedDetection;
  }

  // Default: assume QWERTY (most common worldwide)
  cachedDetection = { detected: "qwerty", method: "language-heuristic", confidence: "low" };
  return cachedDetection;
}

/** Clear cached detection (useful if user changes OS layout mid-session). */
export function resetLayoutDetectionCache(): void {
  cachedDetection = null;
}

/** Get the last cached detection result without re-detecting. */
export function getCachedDetection(): LayoutDetectionResult | null {
  return cachedDetection;
}

/**
 * Resolve effective layout: if setting is "auto", use detected layout;
 * otherwise use the explicit override.
 */
export function resolveEffectiveLayout(
  setting: KeyboardLayout,
  detected: DetectedLayout,
): "qwerty" | "azerty" | "qwertz" {
  if (setting !== "auto") return setting;
  // For "auto", use detected layout. If unknown, default to qwerty
  // (physical scancodes will still be correct).
  return detected === "unknown" ? "qwerty" : detected;
}

// ── VK remapping tables ──────────────────────────────────────────────
//
// These tables remap Windows Virtual Key codes for non-QWERTY layouts.
// The key is the QWERTY VK code (what codeMap currently produces for a
// physical key position), the value is the VK code that corresponds to
// the character that layout produces at that position.
//
// Only keys that differ between layouts are listed.
// Scancodes are never touched — only VK codes change.

// AZERTY: maps physical positions to the VK of the character AZERTY
// produces there. E.g., physical KeyA (QWERTY VK=0x41 'A') on AZERTY
// produces 'Q', so we remap 0x41 → 0x51.
const AZERTY_VK_REMAP: Record<number, number> = {
  0x41: 0x51, // KeyA: A→Q
  0x51: 0x41, // KeyQ: Q→A
  0x57: 0x5A, // KeyW: W→Z
  0x5A: 0x57, // KeyZ: Z→W
  0x4D: 0xBC, // KeyM: M→comma (VK_OEM_COMMA)
};

// QWERTZ: Z/Y swap
const QWERTZ_VK_REMAP: Record<number, number> = {
  0x5A: 0x59, // KeyZ: Z→Y
  0x59: 0x5A, // KeyY: Y→Z
};

/**
 * Remap a VK code based on the effective keyboard layout.
 * Returns the original VK if no remapping is needed (qwerty or unmapped key).
 */
export function remapVkForLayout(
  vk: number,
  effectiveLayout: "qwerty" | "azerty" | "qwertz",
): number {
  if (effectiveLayout === "azerty") {
    return AZERTY_VK_REMAP[vk] ?? vk;
  }
  if (effectiveLayout === "qwertz") {
    return QWERTZ_VK_REMAP[vk] ?? vk;
  }
  return vk;
}
