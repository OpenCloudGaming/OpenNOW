/// <reference types="node" />

import assert from "node:assert/strict";
import test from "node:test";

import type { BrowserWindow } from "electron";
import { normalizeNativeRenderSurface } from "./surface";

test("normalizes renderer bounds into absolute screen coordinates", () => {
  const handle = Buffer.alloc(8);
  handle.writeBigUInt64LE(0x1234n);
  const window = {
    getNativeWindowHandle: () => handle,
    getContentBounds: () => ({ x: 100, y: 200, width: 1280, height: 720 }),
  } as unknown as BrowserWindow;

  const surface = normalizeNativeRenderSurface(window, {
    rect: { x: 12.4, y: 24.6, width: 640.2, height: 360.4 },
    visible: true,
    deviceScaleFactor: 2,
  });

  assert.deepEqual(surface, {
    windowHandle: "0x1234",
    deviceScaleFactor: 2,
    visible: true,
    showStats: false,
    rect: { x: 12, y: 25, width: 640, height: 360 },
    screenRect: { x: 106, y: 212, width: 320, height: 180 },
  });
});

test("uses Windows display-aware DIP conversion without scaling the global monitor origin", () => {
  const handle = Buffer.alloc(8);
  handle.writeBigUInt64LE(0x1234n);
  const window = {
    getNativeWindowHandle: () => handle,
    getContentBounds: () => ({ x: 1920, y: 0, width: 1280, height: 720 }),
  } as unknown as BrowserWindow;

  const surface = normalizeNativeRenderSurface(
    window,
    {
      rect: { x: 150, y: 30, width: 640, height: 360 },
      visible: true,
      deviceScaleFactor: 1.5,
    },
    (point) => ({
      x: 1920 + Math.round((point.x - 1920) * 1.5),
      y: Math.round(point.y * 1.5),
    }),
  );

  assert.deepEqual(surface?.screenRect, {
    x: 2070,
    y: 30,
    width: 640,
    height: 360,
  });
});

test("keeps publishing bounds when Wayland has no embeddable native handle", () => {
  const window = {
    getNativeWindowHandle: () => {
      throw new Error("not supported on Wayland");
    },
    getContentBounds: () => ({ x: 0, y: 0, width: 1280, height: 720 }),
  } as unknown as BrowserWindow;

  const surface = normalizeNativeRenderSurface(window, {
    rect: { x: 0, y: 0, width: 1280, height: 720 },
    visible: true,
    deviceScaleFactor: 2,
  });

  assert.equal(surface?.windowHandle, undefined);
  assert.deepEqual(surface?.screenRect, { x: 0, y: 0, width: 640, height: 360 });
});
