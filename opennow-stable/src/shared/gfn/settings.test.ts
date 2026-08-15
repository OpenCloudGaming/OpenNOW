/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  DEFAULT_SHORTCUT_SETTINGS,
  createDefaultSettings,
  createPlatformShortcutDefaults,
  normalizeRecordingBitrateMbps,
  normalizeRecordingFps,
  normalizeRecordingResolution,
  normalizeGameStreamProfiles,
  resolveGameStreamProfile,
  resolveRuntimePlatform,
} from "./settings";

test("preserves Windows, macOS, and Linux shortcut defaults", () => {
  const windows = createPlatformShortcutDefaults("win32");
  const macOs = createPlatformShortcutDefaults("darwin");
  const linux = createPlatformShortcutDefaults("linux");

  assert.deepEqual(windows.bindings, DEFAULT_SHORTCUT_SETTINGS);
  assert.deepEqual(macOs.bindings, DEFAULT_SHORTCUT_SETTINGS);
  assert.deepEqual(linux.bindings, DEFAULT_SHORTCUT_SETTINGS);
  assert.deepEqual(windows.sidebarToggleAliases, ["Ctrl+G", "Ctrl+Shift+G"]);
  assert.equal(windows.sidebarToggle, "Ctrl+G");
  assert.deepEqual(linux.sidebarToggleAliases, ["Ctrl+G", "Ctrl+Shift+G"]);
  assert.equal(linux.sidebarToggle, "Ctrl+G");
  assert.deepEqual(macOs.sidebarToggleAliases, ["Meta+G"]);
  assert.equal(macOs.sidebarToggle, "Meta+G");
  assert.equal(DEFAULT_SHORTCUT_SETTINGS.shortcutToggleStats, "Ctrl+N");
});

test("resolves main and renderer platform names without environment globals", () => {
  assert.equal(resolveRuntimePlatform("win32"), "win32");
  assert.equal(resolveRuntimePlatform("Win32"), "win32");
  assert.equal(resolveRuntimePlatform("MacIntel"), "darwin");
  assert.equal(resolveRuntimePlatform("Linux x86_64"), "linux");
  assert.equal(resolveRuntimePlatform("plan9"), "unknown");
});

test("creates fresh mutable nested settings defaults", () => {
  const first = createDefaultSettings("win32");
  const second = createDefaultSettings("win32");

  assert.notStrictEqual(first, second);
  assert.notStrictEqual(first.favoriteGameIds, second.favoriteGameIds);
  assert.notStrictEqual(first.videoShader, second.videoShader);
  assert.notStrictEqual(first.gameStreamProfiles, second.gameStreamProfiles);

  first.favoriteGameIds.push("game-1");
  first.videoShader.sharpen = 0;
  first.gameStreamProfiles["game-1"] = {
    resolution: "1280x720",
    fps: 120,
    maxBitrateMbps: 30,
  };

  assert.deepEqual(second.favoriteGameIds, []);
  assert.equal(second.videoShader.sharpen, 40);
  assert.deepEqual(second.gameStreamProfiles, {});
});

test("normalizes persisted per-game stream profiles", () => {
  const persisted = JSON.parse(JSON.stringify({
    overwatch: { resolution: "1280x720", fps: 120, maxBitrateMbps: 35 },
    wukong: { resolution: "3840x2160", fps: 60, maxBitrateMbps: 200 },
    invalid: { resolution: "4K", fps: "fast", maxBitrateMbps: 75 },
  }));

  assert.deepEqual(normalizeGameStreamProfiles(persisted), {
    overwatch: { resolution: "1280x720", fps: 120, maxBitrateMbps: 35 },
    wukong: { resolution: "3840x2160", fps: 60, maxBitrateMbps: 150 },
  });
});

test("resolves a per-game stream profile without changing global defaults", () => {
  const settings = createDefaultSettings("linux");
  settings.gameStreamProfiles.overwatch = {
    resolution: "1280x720",
    fps: 120,
    maxBitrateMbps: 30,
  };

  assert.deepEqual(resolveGameStreamProfile(settings, "overwatch"), {
    resolution: "1280x720",
    fps: 120,
    maxBitrateMbps: 30,
  });
  assert.deepEqual(resolveGameStreamProfile(settings, "wukong"), {
    resolution: "1920x1080",
    fps: 60,
    maxBitrateMbps: 75,
  });
  assert.equal(settings.resolution, "1920x1080");
  assert.equal(settings.fps, 60);
  assert.equal(settings.maxBitrateMbps, 75);
});

test("uses brief recurring Anti-AFK reminder defaults", () => {
  const settings = createDefaultSettings("win32");

  assert.equal(settings.antiAfkReminderEveryMinutes, 15);
  assert.equal(settings.antiAfkReminderDurationSeconds, 5);
});

test("offers the controller mode prompt until the user dismisses it", () => {
  assert.equal(createDefaultSettings("win32").controllerModePromptDismissed, false);
});

test("creates fresh platform shortcut collections", () => {
  const first = createPlatformShortcutDefaults("linux");
  const second = createPlatformShortcutDefaults("linux");

  assert.notStrictEqual(first.bindings, second.bindings);
  assert.notStrictEqual(first.sidebarToggleAliases, second.sidebarToggleAliases);
  first.bindings.shortcutToggleStats = "F4";
  first.sidebarToggleAliases.push("Alt+G");

  assert.equal(second.bindings.shortcutToggleStats, "Ctrl+N");
  assert.deepEqual(second.sidebarToggleAliases, ["Ctrl+G", "Ctrl+Shift+G"]);
});

test("defaults the stats HUD to the bottom-left anchor", () => {
  assert.equal(createDefaultSettings("linux").statsOverlayPosition, "bottom-left");
});

test("uses recording defaults that preserve live stream performance", () => {
  const settings = createDefaultSettings("linux");

  assert.equal(settings.recordingResolution, "720p");
  assert.equal(settings.recordingFps, 30);
  assert.equal(settings.recordingBitrateMbps, null);
});

test("normalizes recording settings to supported performance bounds", () => {
  assert.equal(normalizeRecordingResolution("1080p"), "1080p");
  assert.equal(normalizeRecordingResolution("2160p"), "720p");
  assert.equal(normalizeRecordingResolution(null), "720p");

  assert.equal(normalizeRecordingFps(60), 60);
  assert.equal(normalizeRecordingFps("59"), 60);
  assert.equal(normalizeRecordingFps(45), 30);
  assert.equal(normalizeRecordingFps(Number.NaN), 30);

  assert.equal(normalizeRecordingBitrateMbps(null), null);
  assert.equal(normalizeRecordingBitrateMbps("auto"), null);
  assert.equal(normalizeRecordingBitrateMbps(0), 1);
  assert.equal(normalizeRecordingBitrateMbps(8.4), 8);
  assert.equal(normalizeRecordingBitrateMbps(200), 12);
});
